use super::*;
#[test]
fn desktop_names_and_visibility() {
    let text = "[Desktop Entry]\nType=Application\nName=Editor\nName[fr]=Éditeur\nExec=editor %f\n[Desktop Action New]\nName=Wrong\n";
    assert_eq!(parse(text, Language::French).as_deref(), Some("Éditeur"));
    assert!(
        parse(
            &text.replace("Type=Application", "Type=Application\nHidden=true"),
            Language::English
        )
        .is_none()
    );
    assert!(
        parse(
            &text.replace("Type=Application", "Type=Application\nNoDisplay=true"),
            Language::English
        )
        .is_none()
    );
}
