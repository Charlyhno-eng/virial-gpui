use crate::platform::shell_open;

#[cfg(target_os = "linux")]
#[test]
fn failed_launcher_returns_an_error() {
    let launcher = super::Launcher {
        program: "sh",
        leading: &["-c", "exit 7"],
        detached: false,
    };
    let error = launcher.run(std::path::Path::new("unused")).unwrap_err();
    assert!(error.to_string().contains("7"));
}

#[cfg(target_os = "linux")]
#[test]
fn appimage_becomes_executable_and_runs_with_spaces_in_its_path() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let image = directory.path().join("test application.AppImage");
    // A stand-in executable records its invocation beside itself.
    std::fs::write(&image, "#!/bin/sh\nprintf launched > \"$0.started\"\n").unwrap();
    std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o640)).unwrap();
    shell_open::open(&image).unwrap();
    assert_eq!(
        image.metadata().unwrap().permissions().mode() & 0o777,
        0o740
    );
    let marker = directory.path().join("test application.AppImage.started");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !marker.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "launched");
}

#[cfg(target_os = "linux")]
#[test]
fn package_installer_falls_back_and_receives_the_whole_path() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("local package.deb");
    std::fs::write(&package, "test package").unwrap();
    let installer = directory.path().join("installer");
    std::fs::write(
        &installer,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&installer, std::fs::Permissions::from_mode(0o700)).unwrap();
    let missing = directory.path().join("missing-installer");
    assert!(!super::open_package(&package, &[(missing.to_str().unwrap(), &[])]).unwrap());
    assert!(
        super::open_package(
            &package,
            &[
                (missing.to_str().unwrap(), &[]),
                (installer.to_str().unwrap(), &["--local-filename"]),
            ]
        )
        .unwrap()
    );
    let marker = directory.path().join("installer.args");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !marker.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(marker).unwrap(),
        format!("--local-filename\n{}\n", package.display())
    );
}

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
