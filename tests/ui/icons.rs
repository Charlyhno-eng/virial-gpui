use super::*;

#[test]
fn media_interface_icons_are_embedded_outline_assets() {
    let assets = IconAssets;
    for name in ["image", "music", "video"] {
        let path = format!("icons/{}.svg", interface_icon_name(name));
        let bytes = assets.load(&path).unwrap().unwrap();
        let document = roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let root = document.root_element();
        assert_eq!(root.tag_name().name(), "svg", "{name}");
        assert_eq!(root.attribute("fill"), Some("none"), "{name}");
        assert_eq!(root.attribute("stroke"), Some("black"), "{name}");
        assert!(
            root.children().any(|node| node.has_tag_name("path")),
            "{name}"
        );
        assert!(
            document
                .descendants()
                .all(|node| { node.attribute("fill").is_none_or(|fill| fill == "none") }),
            "{name}"
        );
    }
    assert_eq!(interface_icon_name("folder"), "folder");
}
