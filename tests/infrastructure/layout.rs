use super::*;

fn layout() -> Layout {
    Layout {
        window: Some(Bounds::new(
            point(px(-1250.), px(90.)),
            size(px(1080.), px(720.)),
        )),
        maximized: true,
        fullscreen: false,
        preview_width: Some(385.),
    }
}

#[test]
fn saves_and_restores_window_geometry_and_preview_width() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("virial/layout");
    let mut saved = layout();
    saved.write(&path).unwrap();
    assert_eq!(
        Layout::parse(&fs::read_to_string(&path).unwrap()),
        Some(saved.clone())
    );
    saved.fullscreen = true;
    saved.preview_width = Some(490.);
    saved.write(&path).unwrap();
    assert_eq!(
        Layout::parse(&fs::read_to_string(&path).unwrap()),
        Some(saved)
    );
}

#[test]
fn rejects_invalid_or_non_finite_dimensions() {
    for text in [
        "window=0,0,NaN,580",
        "window=0,inf,900,580",
        "window=0,0,10,580",
        "window=0,0,900,-1",
        "window=0,0,900",
        "preview_width=NaN",
        "preview_width=inf",
        "preview_width=-40",
        "preview_width=100000",
        "maximized=maybe",
        "invalid",
    ] {
        assert!(Layout::parse(text).is_none(), "{text}");
    }
    assert_eq!(Layout::parse(""), Some(Layout::default()));
}
