use super::*;

#[test]
fn reads_localized_user_directories_without_evaluating_shell_commands() {
    let config = "# locations\nXDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\nXDG_DOCUMENTS_DIR=\"/mnt/Documents\"\nXDG_MUSIC_DIR=\"$(touch /tmp/example)\"";
    let home = Path::new("/home/user");
    assert_eq!(
        configured_directory(config, "DOWNLOAD", home),
        Some(home.join("Téléchargements"))
    );
    assert_eq!(
        configured_directory(config, "DOCUMENTS", home),
        Some("/mnt/Documents".into())
    );
    assert!(configured_directory(config, "MUSIC", home).is_none());
}
