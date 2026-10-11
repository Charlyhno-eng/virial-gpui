use crate::platform::shell_open;

#[test]
fn the_launcher_named_by_the_build_is_the_one_this_host_has() {
    // The whole point of the module: no caller may assume a command that is
    // not there. A missing launcher is what made double-clicking a file fail
    // on Windows, where `xdg-open` does not exist.
    let program = shell_open::launcher_program();
    let path = std::env::var("PATH").unwrap_or_default();
    let extensions: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };

    let found = extensions.iter().any(|extension| {
        let candidate = format!("{program}{extension}");
        std::env::split_paths(&path).any(|directory| directory.join(&candidate).is_file())
    });
    assert!(
        found,
        "the launcher this build names ({program}) is not on PATH: opening a file \
         cannot work on this host"
    );
}
