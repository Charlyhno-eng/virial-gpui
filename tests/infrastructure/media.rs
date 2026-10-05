use super::*;

fn wait_for(media: &Media, condition: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let start = Instant::now();
    loop {
        let snapshot = media.snapshot();
        assert!(!snapshot.failed, "media backend failed");
        if condition(&snapshot) {
            return snapshot;
        }
        assert!(
            start.elapsed() < Duration::from_secs(8),
            "media update timed out"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn media_plays_pauses_seeks_and_stops_without_a_window() {
    if Api::load().is_err()
        || std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_err()
    {
        eprintln!("Skipping media integration check: libmpv2 and ffmpeg are required");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    for video in [false, true] {
        let path = root
            .path()
            .join(if video { "clip.mp4" } else { "music.wav" });
        let mut generator = std::process::Command::new("ffmpeg");
        generator.args(["-v", "error", "-f", "lavfi", "-i"]);
        generator.arg(if video {
            "color=c=red:s=160x90:r=25"
        } else {
            "sine=frequency=440:sample_rate=44100"
        });
        generator.args(["-t", "4", "-threads", "1"]).arg(&path);
        assert!(generator.status().unwrap().success());
        let media = Media::new(path, video, None);
        let state = wait_for(&media, |state| {
            state.ready && (!video || state.frame.is_some())
        });
        assert!(state.paused);
        assert!((state.duration - 4.).abs() < 0.2);
        if video {
            // GPUI consumes BGRA. A red source must have red in the third byte.
            let frame = state.frame.unwrap();
            let pixels = frame.as_bytes(0).unwrap();
            let middle = ((540 / 2 * 960 + 960 / 2) * 4) as usize;
            assert!(pixels[middle + 2] > 200);
            assert!(pixels[middle] < 40);
            assert_eq!(pixels[middle + 3], 255);
        }
        media.control(Control::Pause(false));
        wait_for(&media, |state| !state.paused && state.position > 0.15);
        media.control(Control::Pause(true));
        let state = wait_for(&media, |state| state.paused);
        let position = state.position;
        std::thread::sleep(Duration::from_millis(150));
        assert!((media.snapshot().position - position).abs() < 0.1);
        media.control(Control::Seek(2.));
        wait_for(&media, |state| state.position > 2.);
        media.control(Control::Mute(true));
        wait_for(&media, |state| state.muted);
        media.control(Control::Restart);
        wait_for(&media, |state| state.position < 0.1);
        let stop = media.stop.clone();
        let snapshot = media.state.clone();
        drop(media);
        assert!(stop.load(Ordering::Relaxed));
        // The worker releases its shared snapshot only after renderer/core shutdown.
        let start = Instant::now();
        while Arc::strong_count(&snapshot) > 1 {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "decoder did not stop"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn invalid_media_reports_failure_and_releases_archive_copy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid.mp3");
    std::fs::write(&path, b"not media").unwrap();
    let archived = super::super::archive::Materialized {
        path: path.clone(),
        _directory: directory,
    };
    let media = Media::new(path.clone(), false, Some(archived));
    let start = Instant::now();
    while !media.snapshot().failed {
        assert!(start.elapsed() < Duration::from_secs(12));
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(media);
    let start = Instant::now();
    while path.exists() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
}
