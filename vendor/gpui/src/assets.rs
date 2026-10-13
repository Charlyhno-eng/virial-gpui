use crate::{DevicePixels, Pixels, Result, SharedString, Size, size};
use smallvec::SmallVec;

use image::{Delay, Frame};
use std::{
    borrow::Cow,
    fmt,
    hash::Hash,
    io::{Read, Write},
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

/// A source of assets for this app to use.
pub trait AssetSource: 'static + Send + Sync {
    /// Load the given asset from the source path.
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>>;

    /// List the assets at the given path.
    fn list(&self, path: &str) -> Result<Vec<SharedString>>;
}

impl AssetSource for () {
    fn load(&self, _path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(None)
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

/// A unique identifier for the image cache
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ImageId(pub usize);

#[derive(PartialEq, Eq, Hash, Clone)]
pub(crate) struct RenderImageParams {
    pub(crate) image_id: ImageId,
    pub(crate) frame_index: usize,
}

/// A cached and processed image, in BGRA format
pub struct RenderImage {
    /// The ID associated with this image
    pub id: ImageId,
    /// The scale factor of this image on render.
    pub(crate) scale_factor: f32,
    data: SmallVec<[ImageFrame; 1]>,
}

struct ImageFrame {
    width: u32,
    height: u32,
    delay: Delay,
    pixels: ImagePixels,
}

enum ImagePixels {
    Raw(Vec<u8>),
    Deflated { bytes: Vec<u8>, len: usize },
}

impl ImageFrame {
    fn new(frame: Frame, compact: bool) -> Self {
        let (width, height) = frame.buffer().dimensions();
        let delay = frame.delay();
        let bytes = frame.into_buffer().into_raw();
        let len = bytes.len();
        let pixels = if compact && len >= 64 * 1024 {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            if encoder.write_all(&bytes).is_ok()
                && let Ok(compressed) = encoder.finish()
                // Keep raw pixels for noisy frames where compression saves little.
                && compressed.len() < len - len / 8
            {
                ImagePixels::Deflated {
                    bytes: compressed,
                    len,
                }
            } else {
                ImagePixels::Raw(bytes)
            }
        } else {
            ImagePixels::Raw(bytes)
        };
        Self {
            width,
            height,
            delay,
            pixels,
        }
    }

    fn bytes(&self) -> Cow<'_, [u8]> {
        match &self.pixels {
            ImagePixels::Raw(bytes) => Cow::Borrowed(bytes),
            ImagePixels::Deflated { bytes, len } => {
                let mut decoded = Vec::with_capacity(*len);
                flate2::read::DeflateDecoder::new(bytes.as_slice())
                    .read_to_end(&mut decoded)
                    .expect("image pixels were compressed in memory");
                Cow::Owned(decoded)
            }
        }
    }
}

impl PartialEq for RenderImage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for RenderImage {}

impl RenderImage {
    /// Create a new image from the given data.
    pub fn new(data: impl Into<SmallVec<[Frame; 1]>>) -> Self {
        let data = data.into();
        let compact = data.len() > 1;
        Self::from_image_frames(
            data.into_iter()
                .map(|frame| ImageFrame::new(frame, compact))
                .collect(),
        )
    }

    /// Retain asset frames losslessly, compressing each before decoding the next.
    /// Frames must already use BGRA pixels, as for `new`.
    pub fn from_compact_frames(
        frames: impl IntoIterator<Item = image::ImageResult<Frame>>,
    ) -> image::ImageResult<Self> {
        let data = frames
            .into_iter()
            .map(|frame| frame.map(|frame| ImageFrame::new(frame, true)))
            .collect::<image::ImageResult<SmallVec<[ImageFrame; 1]>>>()?;
        Ok(Self::from_image_frames(data))
    }

    fn from_image_frames(data: SmallVec<[ImageFrame; 1]>) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

        Self {
            id: ImageId(NEXT_ID.fetch_add(1, SeqCst)),
            scale_factor: 1.0,
            data,
        }
    }

    /// Access BGRA pixels, decoding compact frames only when their pixels are needed.
    pub fn as_bytes(&self, frame_index: usize) -> Option<Cow<'_, [u8]>> {
        self.data.get(frame_index).map(ImageFrame::bytes)
    }

    /// Get the size of this image, in pixels.
    pub fn size(&self, frame_index: usize) -> Size<DevicePixels> {
        let frame = &self.data[frame_index];
        let (width, height) = (frame.width, frame.height);
        size(width.into(), height.into())
    }

    /// Get the size of this image, in pixels for display, adjusted for the scale factor.
    pub(crate) fn render_size(&self, frame_index: usize) -> Size<Pixels> {
        self.size(frame_index)
            .map(|v| (v.0 as f32 / self.scale_factor).into())
    }

    /// Get the delay of this frame from the previous
    pub fn delay(&self, frame_index: usize) -> Delay {
        self.data[frame_index].delay
    }

    /// Get the number of frames for this image.
    pub fn frame_count(&self) -> usize {
        self.data.len()
    }
}

impl fmt::Debug for RenderImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageData")
            .field("id", &self.id)
            .field("size", &self.size(0))
            .finish()
    }
}
