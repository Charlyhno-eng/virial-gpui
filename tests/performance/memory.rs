//! Run each scenario in its own release process for comparable resident RAM.
use std::{fs, path::Path, sync::Arc, time::Instant};

fn resident_kib() -> usize {
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

#[cfg(target_env = "gnu")]
#[test]
#[ignore = "run separately in release mode to measure freed allocator pages"]
fn idle_allocator_memory_performance() {
    crate::state::memory::release_unused_pages();
    // Small allocations stay in glibc's arenas. Retained blocks prevent the
    // freed ranges from being returned merely by shrinking the arena's end.
    let mut blocks: Vec<_> = (0..400).map(|_| Some(vec![0x5a_u8; 60 * 1024])).collect();
    let allocated = resident_kib();
    for (index, block) in blocks.iter_mut().enumerate() {
        if index % 16 != 0 {
            *block = None;
        }
    }
    let freed = resident_kib();
    let started = Instant::now();
    // The application reclaims pages on a worker, away from the UI thread.
    std::thread::spawn(crate::state::memory::release_unused_pages)
        .join()
        .unwrap();
    let reclaimed = resident_kib();
    println!(
        "MEMORY idle_allocator: allocated_kib={allocated} freed_kib={freed} reclaimed_kib={reclaimed} trim_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.
    );
    for block in blocks.iter().flatten() {
        assert!(block.iter().all(|byte| *byte == 0x5a));
    }
    assert!(reclaimed < freed, "freed resident pages should be returned");
}

#[test]
fn compact_animation_preserves_every_pixel_dimensions_and_delay() {
    let expected: Vec<_> = (0..3)
        .map(|index| {
            image::Frame::from_parts(
                image::RgbaImage::from_pixel(160, 120, image::Rgba([index, 32, 200, 0])),
                0,
                0,
                image::Delay::from_numer_denom_ms(17 + u32::from(index), 3),
            )
        })
        .collect();
    let animation =
        gpui::RenderImage::from_compact_frames(expected.iter().cloned().map(Ok)).unwrap();
    assert_eq!(animation.frame_count(), expected.len());
    for (index, frame) in expected.iter().enumerate() {
        let bytes = animation.as_bytes(index).unwrap();
        assert!(
            matches!(bytes, std::borrow::Cow::Owned(_)),
            "compressible frames should be compact"
        );
        assert_eq!(bytes.as_ref(), frame.buffer().as_raw());
        assert_eq!(animation.size(index).width.0, 160);
        assert_eq!(animation.size(index).height.0, 120);
        assert_eq!(animation.delay(index), frame.delay());
    }
    assert!(animation.as_bytes(expected.len()).is_none());
}

#[test]
fn noisy_frames_and_single_images_keep_raw_pixels() {
    let mut seed = 0x12345678_u32;
    let pixels: Vec<_> = (0..160 * 120 * 4)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect();
    let frame = image::Frame::new(image::RgbaImage::from_raw(160, 120, pixels.clone()).unwrap());
    let animation = gpui::RenderImage::from_compact_frames([Ok(frame.clone())]).unwrap();
    assert!(matches!(
        animation.as_bytes(0),
        Some(std::borrow::Cow::Borrowed(_))
    ));
    assert_eq!(animation.as_bytes(0).unwrap().as_ref(), pixels);
    let still = gpui::RenderImage::new(vec![frame]);
    assert!(matches!(
        still.as_bytes(0),
        Some(std::borrow::Cow::Borrowed(_))
    ));
    assert_eq!(still.as_bytes(0).unwrap().as_ref(), pixels);
}

#[test]
fn compact_animation_propagates_decoder_errors() {
    let frames = [
        Ok(image::Frame::new(image::RgbaImage::new(1, 1))),
        Err(image::ImageError::IoError(std::io::Error::other(
            "broken frame",
        ))),
    ];
    assert!(gpui::RenderImage::from_compact_frames(frames).is_err());
}

#[test]
fn decoded_gif_keeps_composited_transparency_and_timing() {
    use image::{
        AnimationDecoder,
        codecs::gif::{GifDecoder, GifEncoder, Repeat},
    };
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for index in 0..3 {
            let mut pixels = image::RgbaImage::new(160, 120);
            for x in index * 20..index * 20 + 20 {
                for y in 10..60 {
                    pixels.put_pixel(x, y, image::Rgba([200, 32, 64, 255]));
                }
            }
            encoder
                .encode_frame(image::Frame::from_parts(
                    pixels,
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(40 + index * 10, 1),
                ))
                .unwrap();
        }
    }
    let decode = || {
        GifDecoder::new(std::io::Cursor::new(&bytes))
            .unwrap()
            .into_frames()
    };
    let expected = decode().collect_frames().unwrap();
    let animation = gpui::RenderImage::from_compact_frames(decode()).unwrap();
    for (index, frame) in expected.iter().enumerate() {
        assert_eq!(
            animation.as_bytes(index).unwrap().as_ref(),
            frame.buffer().as_raw()
        );
        assert_eq!(animation.delay(index), frame.delay());
    }
}

#[test]
fn decoded_webp_keeps_composited_transparency_and_timing() {
    use image::{AnimationDecoder, codecs::webp::WebPDecoder};
    let images: Vec<_> = (0..3)
        .map(|index| {
            let mut pixels = image::RgbaImage::new(160, 120);
            for x in index * 20..index * 20 + 20 {
                for y in 10..60 {
                    pixels.put_pixel(x, y, image::Rgba([200, 32, 64, 255]));
                }
            }
            pixels
        })
        .collect();
    let mut config = webp::WebPConfig::new().unwrap();
    config.lossless = 1;
    let mut encoder = webp::AnimEncoder::new(160, 120, &config);
    for (index, pixels) in images.iter().enumerate() {
        encoder.add_frame(webp::AnimFrame::from_rgba(
            pixels.as_raw(),
            160,
            120,
            index as i32 * 40,
        ));
    }
    let bytes = encoder.try_encode().unwrap();
    let decode = || {
        let mut decoder = WebPDecoder::new(std::io::Cursor::new(&*bytes)).unwrap();
        assert!(decoder.has_animation());
        decoder
            .set_background_color(image::Rgba([0, 0, 0, 0]))
            .unwrap();
        decoder.into_frames()
    };
    let expected = decode().collect_frames().unwrap();
    assert_eq!(expected.len(), 3);
    let animation = gpui::RenderImage::from_compact_frames(decode()).unwrap();
    for (index, frame) in expected.iter().enumerate() {
        assert_eq!(
            animation.as_bytes(index).unwrap().as_ref(),
            frame.buffer().as_raw()
        );
        assert_eq!(animation.delay(index), frame.delay());
    }
}

#[test]
#[ignore = "run separately with --release --ignored --nocapture to measure RAM"]
#[allow(clippy::assertions_on_constants)]
fn editor_memory_performance() {
    assert!(!cfg!(debug_assertions), "Run with --release");
    use crate::{
        infrastructure::editor_io::{DocumentFormat, LineEnding},
        state::editor::Editor,
    };
    let before = resident_kib();
    let started = Instant::now();
    let source = "let café = 42; // undo keeps this text\n".repeat(40_000);
    let mut editor = Editor::new(
        &source,
        DocumentFormat {
            line_ending: LineEnding::Lf,
            bom: false,
            latin1_fallback: false,
        },
    );
    for version in 0..60 {
        editor.select_all();
        editor.insert(&format!("// version {version}\n{source}"));
    }
    println!(
        "MEMORY editor_60_large_replacements resident_delta_kib={} build_ms={:.1}",
        resident_kib().saturating_sub(before),
        started.elapsed().as_secs_f64() * 1000.
    );
    for _ in 0..60 {
        editor.undo();
    }
    assert_eq!(editor.text(), source);
    for _ in 0..60 {
        editor.redo();
    }
    assert_eq!(editor.text(), format!("// version 59\n{source}"));
    std::hint::black_box(editor);
}

#[test]
#[ignore = "run separately with --release --ignored --nocapture to measure RAM"]
#[allow(clippy::assertions_on_constants)]
fn code_memory_performance() {
    assert!(!cfg!(debug_assertions), "Run with --release");
    use crate::state::code_preview::CodePreview;
    let source = "let café = 42; // retained syntax colors\n".repeat(40_000);
    // Warm the grammar set separately from per-document allocations.
    drop(CodePreview::new(Path::new("main.rs"), "fn main() {}"));
    let before = resident_kib();
    let code = CodePreview::new(Path::new("main.rs"), &source).unwrap();
    println!(
        "MEMORY code_40000_lines resident_delta_kib={}",
        resident_kib().saturating_sub(before)
    );
    assert_eq!(code.lines.len(), 40_001);
    std::hint::black_box(code);
}

#[test]
#[ignore = "run separately with --release --ignored --nocapture to measure RAM"]
#[allow(clippy::assertions_on_constants)]
fn animation_memory_performance() {
    assert!(!cfg!(debug_assertions), "Run with --release");
    let before = resident_kib();
    let started = Instant::now();
    let frames = (0..60)
        .map(|index| {
            image::Frame::from_parts(
                image::RgbaImage::from_pixel(960, 540, image::Rgba([index, 64, 128, 255])),
                0,
                0,
                image::Delay::from_numer_denom_ms(40, 1),
            )
        })
        .collect::<Vec<_>>();
    let animation = Arc::new(gpui::RenderImage::new(frames));
    println!(
        "MEMORY animation_60_frames resident_delta_kib={} build_ms={:.1}",
        resident_kib().saturating_sub(before),
        started.elapsed().as_secs_f64() * 1000.
    );
    assert_eq!(animation.frame_count(), 60);
    for index in 0..60 {
        assert_eq!(
            &animation.as_bytes(index).unwrap()[..4],
            &[index as u8, 64, 128, 255]
        );
        assert_eq!(
            animation.delay(index),
            image::Delay::from_numer_denom_ms(40, 1)
        );
    }
    std::hint::black_box(animation);
}
