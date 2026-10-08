use super::*;
use crate::infrastructure::{operations::Operation, undo};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink};
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir as symlink_dir_win, symlink_file as symlink_file_win};
#[cfg(windows)]
fn symlink<P: AsRef<std::path::Path>, Q: AsRef<std::path::Path>>(
    original: P,
    link: Q,
) -> std::io::Result<()> {
    if std::fs::metadata(&original).map(|m| m.is_dir()).unwrap_or(false) {
        symlink_dir_win(original, link)
    } else {
        symlink_file_win(original, link)
    }
}

fn fixture(root: &Path) -> PathBuf {
    let source = root.join("photo with spaces.png");
    let image = image::RgbaImage::from_fn(8, 8, |x, _| {
        if x < 4 {
            image::Rgba([220, 40, 60, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    });
    image.save(&source).unwrap();
    source
}

#[test]
fn converts_real_images_preserves_alpha_and_original_and_flattens_jpeg_on_white() {
    let root = tempfile::tempdir().unwrap();
    let source = fixture(root.path());
    let original = fs::read(&source).unwrap();
    for format in ExportFormat::ALL {
        let edit = ImageEdit::Convert(format);
        let name = edit.suggested_name(&source);
        export(&source, &name, edit).unwrap();
        let bytes = fs::read(root.path().join(&name)).unwrap();
        assert_eq!(image::guess_format(&bytes).unwrap(), format.image_format());
        let result = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(result.dimensions(), (8, 8));
        if format == ExportFormat::Jpeg {
            let white = result.get_pixel(7, 7);
            assert!(white.0[..3].iter().all(|value| *value > 240));
            assert_eq!(white[3], 255);
        } else {
            if format == ExportFormat::Png {
                assert_eq!(result.get_pixel(0, 0).0, [220, 40, 60, 255]);
            } else {
                let pixel = result.get_pixel(0, 0);
                for (actual, expected) in pixel.0[..3].iter().zip([220i16, 40, 60]) {
                    assert!((i16::from(*actual) - expected).abs() < 20);
                }
            }
            assert_eq!(result.get_pixel(7, 7)[3], 0);
        }
    }
    assert_eq!(fs::read(source).unwrap(), original);
}

#[test]
fn accepts_jpeg_webp_bmp_and_gif_sources_and_exports_gif_first_frame() {
    let root = tempfile::tempdir().unwrap();
    let image = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 8, image::Rgb([255, 0, 0])));
    let edit = ImageEdit::Convert(ExportFormat::Png);
    for format in [ImageFormat::Jpeg, ImageFormat::WebP, ImageFormat::Bmp] {
        let source = root
            .path()
            .join(format!("source.{}", format.extensions_str()[0]));
        image.save_with_format(&source, format).unwrap();
        let name = edit.suggested_name(&source);
        export(&source, &name, edit).unwrap();
        assert_eq!(image::open(root.path().join(&name)).unwrap().width(), 8);
        fs::remove_file(root.path().join(name)).unwrap();
    }
    let source = root.path().join("animated.gif");
    let mut encoder = image::codecs::gif::GifEncoder::new(File::create(&source).unwrap());
    for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
        encoder
            .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                8,
                8,
                image::Rgba(color),
            )))
            .unwrap();
    }
    drop(encoder);
    export(&source, "first.png", edit).unwrap();
    assert_eq!(
        image::open(root.path().join("first.png"))
            .unwrap()
            .to_rgb8()
            .get_pixel(0, 0)
            .0,
        [255, 0, 0]
    );
}

#[test]
fn rejects_conflicts_symlinks_invalid_names_and_invalid_images_without_partial_files() {
    let root = tempfile::tempdir().unwrap();
    let source = fixture(root.path());
    let edit = ImageEdit::Convert(ExportFormat::Png);
    fs::write(root.path().join("existing.png"), "keep").unwrap();
    assert!(export(&source, "existing.png", edit).is_err());
    assert_eq!(fs::read(root.path().join("existing.png")).unwrap(), b"keep");
    symlink("missing.png", root.path().join("link.png")).unwrap();
    assert!(export(&source, "link.png", edit).is_err());
    assert_eq!(
        fs::read_link(root.path().join("link.png")).unwrap(),
        Path::new("missing.png")
    );
    for name in ["../escape.png", "", "wrong.jpg", "nested/out.png"] {
        assert!(export(&source, name, edit).is_err());
    }
    assert!(export(&source, source.file_name().unwrap().to_str().unwrap(), edit).is_err());
    fs::write(&source, b"not an image").unwrap();
    assert!(export(&source, "invalid.png", edit).is_err());
    assert!(!root.path().join("invalid.png").exists());
    assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".virial-image-")
    }));
}

#[test]
fn rejects_oversized_inputs_before_processing() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("large.png");
    File::create(&source)
        .unwrap()
        .set_len(INPUT_LIMIT + 1)
        .unwrap();
    assert!(export(&source, "out.png", ImageEdit::Convert(ExportFormat::Png)).is_err());
    // A BMP header describes huge dimensions without allocating its pixel buffer.
    let mut header = vec![0; 54];
    header[..2].copy_from_slice(b"BM");
    header[10..14].copy_from_slice(&54u32.to_le_bytes());
    header[14..18].copy_from_slice(&40u32.to_le_bytes());
    header[18..22].copy_from_slice(&8192u32.to_le_bytes());
    header[22..26].copy_from_slice(&8192u32.to_le_bytes());
    header[26..28].copy_from_slice(&1u16.to_le_bytes());
    header[28..30].copy_from_slice(&24u16.to_le_bytes());
    fs::write(&source, header).unwrap();
    let error = export(&source, "out.png", ImageEdit::Convert(ExportFormat::Png)).unwrap_err();
    assert!(error.to_string().contains("too large"), "{error}");
    assert!(!root.path().join("out.png").exists());
}

fn remover(root: &Path, body: &str) -> PathBuf {
    let executable = root.join("mock-rembg");
    fs::write(&executable, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    executable
}

#[test]
fn background_removal_uses_local_cli_and_publishes_valid_transparent_png() {
    let root = tempfile::tempdir().unwrap();
    let source = fixture(root.path());
    // Validate the CLI contract independently of model downloads/inference.
    let executable = remover(
        root.path(),
        "test \"$1\" = i && test \"$2\" = -m && test \"$3\" = u2netp && test \"$4\" = -- || exit 2\ncp -- \"$5\" \"$6\"",
    );
    export_with_rembg(
        &source,
        "cutout.png",
        ImageEdit::RemoveBackground,
        &executable,
    )
    .unwrap();
    let result = image::open(root.path().join("cutout.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(result.get_pixel(7, 7)[3], 0);
    assert_eq!(result.get_pixel(0, 0).0, [220, 40, 60, 255]);
}

#[test]
fn missing_failing_or_invalid_background_remover_does_not_publish_output() {
    let root = tempfile::tempdir().unwrap();
    let source = fixture(root.path());
    let error = export_with_rembg(
        &source,
        "out.png",
        ImageEdit::RemoveBackground,
        &root.path().join("missing"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("rembg[cpu,cli]"));
    for body in [
        "echo model-download-failed >&2; exit 1",
        "printf garbage > \"$6\"",
    ] {
        let executable = remover(root.path(), body);
        assert!(
            export_with_rembg(&source, "out.png", ImageEdit::RemoveBackground, &executable)
                .is_err()
        );
        assert!(!root.path().join("out.png").exists());
    }
}

#[test]
fn exports_are_undoable_on_disk_and_inside_zip_without_changing_originals() {
    let root = tempfile::tempdir().unwrap();
    let source = fixture(root.path());
    let original = fs::read(&source).unwrap();
    let data = root.path().join("data");
    let edit = ImageEdit::Convert(ExportFormat::Webp);
    undo::execute(
        &data,
        Operation::ImageExport {
            source: source.clone(),
            name: "out.webp".into(),
            edit,
        },
    )
    .unwrap();
    assert!(root.path().join("out.webp").exists());
    assert!(undo::undo(&data).unwrap());
    assert!(!root.path().join("out.webp").exists());
    assert_eq!(fs::read(&source).unwrap(), original);

    let zip = root.path().join("images.zip");
    let mut writer = zip::ZipWriter::new(File::create(&zip).unwrap());
    writer
        .start_file(
            "folder/original.png",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer.write_all(&original).unwrap();
    writer.finish().unwrap();
    let zip_original = fs::read(&zip).unwrap();
    let member = zip.join("folder/original.png");
    undo::execute(
        &data,
        Operation::ImageExport {
            source: member.clone(),
            name: "out.webp".into(),
            edit,
        },
    )
    .unwrap();
    let output = archive::materialize(&zip.join("folder/out.webp"), INPUT_LIMIT).unwrap();
    assert_eq!(image::open(&output.path).unwrap().width(), 8);
    assert_eq!(
        archive::read_prefix(&member, INPUT_LIMIT).unwrap(),
        original
    );
    assert!(export(&member, "out.webp", edit).is_err());
    assert!(undo::undo(&data).unwrap());
    assert_eq!(fs::read(zip).unwrap(), zip_original);
}

#[test]
fn optimized_encoders_reduce_size_without_resizing() {
    let root = tempfile::tempdir().unwrap();
    let image = image::RgbaImage::from_fn(256, 256, |x, y| {
        let noise = ((x * 73 + y * 131 + x * y * 17) % 31) as u8;
        image::Rgba([x as u8 / 2 + noise, y as u8 / 2 + noise, 100 + noise, 255])
    });
    let source = root.path().join("source.png");
    image.save(&source).unwrap();
    let baseline_png = fs::metadata(&source).unwrap().len();
    let baseline_webp = root.path().join("lossless.webp");
    image.save(&baseline_webp).unwrap();
    export(
        &source,
        "optimized.png",
        ImageEdit::Convert(ExportFormat::Png),
    )
    .unwrap();
    export(
        &source,
        "optimized.webp",
        ImageEdit::Convert(ExportFormat::Webp),
    )
    .unwrap();
    let png = image::open(root.path().join("optimized.png")).unwrap();
    assert_eq!(png.color(), image::ColorType::Rgb8);
    assert_eq!(png.to_rgba8(), image);
    assert!(
        fs::metadata(root.path().join("optimized.png"))
            .unwrap()
            .len()
            < baseline_png
    );
    assert!(
        fs::metadata(root.path().join("optimized.webp"))
            .unwrap()
            .len()
            < fs::metadata(baseline_webp).unwrap().len() / 2
    );
    assert_eq!(
        image::open(root.path().join("optimized.webp"))
            .unwrap()
            .width(),
        256
    );
}

#[test]
fn png_optimization_reduces_conversion_and_cutout_sizes_without_changing_pixels() {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};

    let root = tempfile::tempdir().unwrap();
    let executable = remover(root.path(), "cp -- \"$5\" \"$6\"");
    for transparent in [false, true] {
        // A limited palette scattered across the image benefits from indexed PNG.
        // Include partial alpha and nonzero RGB under fully transparent pixels.
        let pixels = image::RgbaImage::from_fn(256, 256, |x, y| {
            let value = ((x * 73 + y * 131 + x * y * 17) % 64) as u8;
            image::Rgba([
                value * 3,
                value * 2,
                255 - value,
                if transparent { (value % 4) * 85 } else { 255 },
            ])
        });
        let image = if transparent {
            DynamicImage::ImageRgba8(pixels.clone())
        } else {
            DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(pixels.clone()).into_rgb8())
        };
        let mut baseline = Vec::new();
        PngEncoder::new_with_quality(&mut baseline, CompressionType::Best, FilterType::Adaptive)
            .write_image(image.as_bytes(), 256, 256, image.color().into())
            .unwrap();
        let source = root.path().join("source.png");
        fs::write(&source, &baseline).unwrap();
        let edit = if transparent {
            ImageEdit::RemoveBackground
        } else {
            ImageEdit::Convert(ExportFormat::Png)
        };
        let name = if transparent {
            "cutout.png"
        } else {
            "converted.png"
        };
        export_with_rembg(&source, name, edit, &executable).unwrap();
        let output = fs::read(root.path().join(name)).unwrap();
        assert_eq!(image::load_from_memory(&output).unwrap().to_rgba8(), pixels);
        assert!(
            output.len() < baseline.len() * 3 / 4,
            "{edit:?}: {} bytes versus {} before optimization",
            output.len(),
            baseline.len()
        );
        assert_eq!(fs::read(&source).unwrap(), baseline);
    }
}

#[test]
fn png_preserves_grayscale_and_sixteen_bit_transparency() {
    let root = tempfile::tempdir().unwrap();
    let gray = DynamicImage::ImageLuma16(image::ImageBuffer::from_pixel(
        8,
        8,
        image::Luma([12345u16]),
    ));
    encode(
        gray.clone(),
        &root.path().join("gray.png"),
        ExportFormat::Png,
    )
    .unwrap();
    let result = image::open(root.path().join("gray.png")).unwrap();
    assert_eq!(result.color(), image::ColorType::L16);
    assert_eq!(result.as_bytes(), gray.as_bytes());
    // This alpha rounds to opaque at 8 bits; it must still be preserved at 16 bits.
    let rgba = DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(
        8,
        8,
        image::Rgba([12345u16, 23456, 34567, 65534]),
    ));
    encode(
        rgba.clone(),
        &root.path().join("alpha.png"),
        ExportFormat::Png,
    )
    .unwrap();
    let result = image::open(root.path().join("alpha.png")).unwrap();
    assert_eq!(result.color(), image::ColorType::Rgba16);
    assert_eq!(result.as_bytes(), rgba.as_bytes());
}

#[test]
fn managed_rembg_is_found_without_path_changes_and_falls_back_to_path() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(rembg_executable(Some(root.path())), PathBuf::from("rembg"));
    let installed = root.path().join("virial/rembg-venv/bin/rembg");
    fs::create_dir_all(installed.parent().unwrap()).unwrap();
    fs::write(&installed, "#!/bin/sh\n").unwrap();
    assert_eq!(rembg_executable(Some(root.path())), installed);
    assert_eq!(rembg_executable(None), PathBuf::from("rembg"));
}
