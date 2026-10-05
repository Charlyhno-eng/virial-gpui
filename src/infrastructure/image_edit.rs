//! Raster image exports; originals are preserved and outputs never replace files.
use super::{archive, operations};
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader};
use std::{
    fs::{self, File},
    io::{self, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(test)]
use image::ImageFormat;

const INPUT_LIMIT: u64 = 20 * 1024 * 1024;
const PIXEL_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    Png,
    Jpeg,
    Webp,
}

impl ExportFormat {
    pub(crate) const ALL: [Self; 3] = [Self::Png, Self::Jpeg, Self::Webp];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Webp => "WebP",
        }
    }

    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    #[cfg(test)]
    fn image_format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Webp => ImageFormat::WebP,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageEdit {
    Convert(ExportFormat),
    RemoveBackground,
}

impl ImageEdit {
    pub(crate) fn format(self) -> ExportFormat {
        match self {
            Self::Convert(format) => format,
            Self::RemoveBackground => ExportFormat::Png,
        }
    }

    pub(crate) fn suggested_name(self, source: &Path) -> String {
        let stem = source.file_stem().unwrap_or_default().to_string_lossy();
        let suffix = match self {
            Self::Convert(_) => "converted",
            Self::RemoveBackground => "no-bg",
        };
        format!("{stem}-{suffix}.{}", self.format().extension())
    }
}

pub(crate) fn destination(source: &Path, name: &str, edit: ImageEdit) -> io::Result<PathBuf> {
    let directory = source
        .parent()
        .ok_or_else(|| io::Error::other("No parent directory"))?;
    let path = operations::named_path(directory, name)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let matches = extension.eq_ignore_ascii_case(edit.format().extension())
        || (edit.format() == ExportFormat::Jpeg && extension.eq_ignore_ascii_case("jpeg"));
    if !matches {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The file extension must match the selected format",
        ));
    }
    Ok(path)
}

fn decode(path: &Path, byte_limit: u64) -> io::Result<DynamicImage> {
    if !fs::metadata(path)?.is_file() {
        return Err(io::Error::other("Image must be a regular file"));
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > byte_limit {
        return Err(io::Error::other(format!(
            "Image must be a regular file no larger than {} MiB",
            byte_limit / (1024 * 1024)
        )));
    }
    let mut reader = ImageReader::new(BufReader::new(file)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(io::Error::other)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > PIXEL_LIMIT
        || decoder.total_bytes() > 256 * 1024 * 1024
    {
        return Err(io::Error::other(
            "Image is too large to process (maximum 32 megapixels)",
        ));
    }
    let orientation = decoder.orientation().map_err(io::Error::other)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(io::Error::other)?;
    image.apply_orientation(orientation);
    Ok(image)
}

fn has_transparency(image: &DynamicImage) -> bool {
    match image {
        DynamicImage::ImageRgba8(buffer) => buffer.pixels().any(|pixel| pixel[3] != 255),
        DynamicImage::ImageLumaA8(buffer) => buffer.pixels().any(|pixel| pixel[1] != 255),
        DynamicImage::ImageRgba16(buffer) => buffer.pixels().any(|pixel| pixel[3] != u16::MAX),
        DynamicImage::ImageLumaA16(buffer) => buffer.pixels().any(|pixel| pixel[1] != u16::MAX),
        _ => image.color().has_alpha(),
    }
}

fn encode(image: DynamicImage, path: &Path, format: ExportFormat) -> io::Result<()> {
    // JPEG has no alpha channel. Composite onto white instead of turning transparent pixels black.
    let image = if format == ExportFormat::Jpeg {
        let rgba = image.into_rgba8();
        let rgb = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
            let pixel = rgba.get_pixel(x, y);
            let alpha = u32::from(pixel[3]);
            image::Rgb([0, 1, 2].map(|channel| {
                ((u32::from(pixel[channel]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8
            }))
        });
        DynamicImage::ImageRgb8(rgb)
    } else if has_transparency(&image) {
        image
    } else if image.color().has_alpha() {
        // Opaque images need no alpha channel; retain grayscale and 16-bit PNG data.
        match image {
            DynamicImage::ImageLumaA8(_) => DynamicImage::ImageLuma8(image.into_luma8()),
            DynamicImage::ImageLumaA16(_) => DynamicImage::ImageLuma16(image.into_luma16()),
            DynamicImage::ImageRgba16(_) => DynamicImage::ImageRgb16(image.into_rgb16()),
            _ => DynamicImage::ImageRgb8(image.into_rgb8()),
        }
    } else {
        image
    };
    match format {
        ExportFormat::Png => {
            use image::codecs::png::{CompressionType, FilterType, PngEncoder};
            let mut bytes = Vec::new();
            PngEncoder::new_with_quality(&mut bytes, CompressionType::Best, FilterType::Adaptive)
                .write_image(
                    image.as_bytes(),
                    image.width(),
                    image.height(),
                    image.color().into(),
                )
                .map_err(io::Error::other)?;
            // Try lossless palette/bit-depth reductions and alternative PNG filters.
            // Keep hidden RGB values and 16-bit precision as well as visible pixels.
            let mut options = oxipng::Options::from_preset(3);
            options.timeout = Some(std::time::Duration::from_secs(10));
            let optimized =
                oxipng::optimize_from_memory(&bytes, &options).map_err(io::Error::other)?;
            // Optimization must never inflate the already compressed export.
            fs::write(
                path,
                if optimized.len() < bytes.len() {
                    &optimized
                } else {
                    &bytes
                },
            )
        }
        ExportFormat::Jpeg => {
            image::codecs::jpeg::JpegEncoder::new_with_quality(File::create(path)?, 75)
                .encode_image(&image)
                .map_err(io::Error::other)
        }
        ExportFormat::Webp => {
            // The image crate only encodes lossless WebP, which can inflate photos.
            let rgba = image.into_rgba8();
            let bytes = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height())
                .encode_simple(false, 80.0)
                .map_err(|error| io::Error::other(format!("WebP encoding failed: {error:?}")))?;
            fs::write(path, &*bytes)
        }
    }
}

fn remove_background(image: DynamicImage, output: &Path, executable: &Path) -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("input.png");
    let result = directory.path().join("output.png");
    let dimensions = (image.width(), image.height());
    encode(image, &input, ExportFormat::Png)?;
    let log = directory.path().join("stderr");
    let status = Command::new(executable)
        .args(["i", "-m", "u2netp", "--"])
        .arg(&input)
        .arg(&result)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(File::create(&log)?)
        .status()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                io::Error::new(error.kind(), "Background removal requires rembg[cpu,cli]. Run ./install.sh --with-background-removal as described in README.md")
            } else {
                error
            }
        })?;
    if !status.success() {
        let mut detail = Vec::new();
        File::open(log)?.take(8192).read_to_end(&mut detail)?;
        return Err(io::Error::other(format!(
            "Background removal failed ({status}): {}",
            String::from_utf8_lossy(&detail).trim()
        )));
    }
    // A lossless PNG can be larger than the compressed source; keep pixel limits on the result.
    let image = decode(&result, 256 * 1024 * 1024)?;
    if (image.width(), image.height()) != dimensions || !image.color().has_alpha() {
        return Err(io::Error::other(
            "Background remover returned an invalid transparent image",
        ));
    }
    encode(image, output, ExportFormat::Png)
}

fn rembg_executable(data_home: Option<&Path>) -> PathBuf {
    let installed = data_home.map(|data| data.join("virial/rembg-venv/bin/rembg"));
    installed
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("rembg"))
}

pub(crate) fn export(source: &Path, name: &str, edit: ImageEdit) -> io::Result<()> {
    let data_home = std::env::var_os("HOME").map(|home| super::recent::data_home(Path::new(&home)));
    let executable = rembg_executable(data_home.as_deref());
    export_with_rembg(source, name, edit, &executable)
}

fn export_with_rembg(
    source: &Path,
    name: &str,
    edit: ImageEdit,
    executable: &Path,
) -> io::Result<()> {
    let target = destination(source, name, edit)?;
    let directory = target
        .parent()
        .ok_or_else(|| io::Error::other("No parent directory"))?;
    let exists = if archive::is_member(&target) {
        archive::entry(&target).map(|_| ())
    } else {
        fs::symlink_metadata(&target).map(|_| ())
    };
    match exists {
        Ok(()) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", target.display()),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let extracted = if archive::is_member(source) {
        Some(archive::materialize(source, INPUT_LIMIT)?)
    } else {
        None
    };
    let image = decode(
        extracted
            .as_ref()
            .map(|file| file.path.as_path())
            .unwrap_or(source),
        INPUT_LIMIT,
    )?;
    // Stage the complete image before publishing it. Failed decoding/inference leaves no output.
    let staging = if archive::split(directory).is_some() {
        tempfile::tempdir()?
    } else {
        tempfile::Builder::new()
            .prefix(".virial-image-")
            .tempdir_in(directory)?
    };
    let output = staging.path().join(name);
    match edit {
        ImageEdit::Convert(format) => encode(image, &output, format)?,
        ImageEdit::RemoveBackground => remove_background(image, &output, executable)?,
    }
    File::open(&output)?.sync_all()?;
    if archive::split(directory).is_some() {
        archive::transfer(vec![output], directory.to_path_buf(), false)
    } else {
        operations::rename(&output, &target)?;
        File::open(directory)?.sync_all()
    }
}

#[cfg(test)]
#[path = "../../tests/infrastructure/image_edit.rs"]
mod tests;
