use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = env::temp_dir().join(format!(
            "virial-desktop-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn uses_the_packaged_launcher_only_for_its_matching_binary() {
    let fixture = Fixture::new();
    let prefix = fixture.0.join("usr");
    let binary = prefix.join("bin/virial-gpui");
    let data = prefix.join("share");
    assert!(!system_registered(&binary, &prefix));
    install(&data, &binary).unwrap();
    assert!(system_registered(&binary, &prefix));
    assert!(!system_registered(
        &fixture.0.join("Downloads/virial-gpui"),
        &prefix
    ));
    assert!(!system_registered(&prefix.join("bin/another-app"), &prefix));
    fs::remove_file(data.join("applications/virial-gpui.desktop")).unwrap();
    assert!(!system_registered(&binary, &prefix));
    install(&data, &binary).unwrap();
    fs::remove_file(data.join("icons/hicolor/512x512/apps/virial-gpui.png")).unwrap();
    assert!(!system_registered(&binary, &prefix));
}

#[test]
fn registers_standalone_logo_and_matching_desktop_identity() {
    let fixture = Fixture::new();
    let data = fixture.0.join("user data");
    let binary = fixture.0.join("Downloads/Virial app");
    assert!(install(&data, &binary).unwrap());

    let icon = data.join("icons/hicolor/512x512/apps/virial-gpui.png");
    assert_eq!(fs::read(&icon).unwrap(), LOGO);
    assert!(LOGO.starts_with(b"\x89PNG\r\n\x1a\n"));
    let desktop = fs::read_to_string(data.join(format!("applications/{APP_ID}.desktop"))).unwrap();
    assert!(desktop.contains(&format!("Exec=\"{}\"\n", binary.display())));
    assert!(desktop.contains(&format!("Icon={}\n", icon.display())));
    assert!(desktop.contains(&format!("StartupWMClass={APP_ID}\n")));
    assert!(desktop.contains("Terminal=false\n"));
}

#[test]
fn repairs_missing_assets_and_updates_a_moved_executable() {
    let fixture = Fixture::new();
    let first = fixture.0.join("Downloads/virial-gpui");
    install(&fixture.0, &first).unwrap();
    let icon = fixture.0.join("icons/hicolor/512x512/apps/virial-gpui.png");
    fs::remove_file(&icon).unwrap();
    let moved = fixture.0.join("Applications/virial-gpui");
    assert!(install(&fixture.0, &moved).unwrap());
    assert_eq!(fs::read(&icon).unwrap(), LOGO);
    let launcher = fixture.0.join("applications/virial-gpui.desktop");
    let desktop = fs::read_to_string(&launcher).unwrap();
    assert!(desktop.contains(&format!("Exec=\"{}\"\n", moved.display())));
    assert!(!desktop.contains(&first.to_string_lossy().to_string()));

    // Reopening an unchanged binary must not trigger desktop cache updates.
    let times = fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH);
    for path in [&icon, &launcher] {
        fs::File::open(path).unwrap().set_times(times).unwrap();
    }
    assert!(!install(&fixture.0, &moved).unwrap());
    for path in [&icon, &launcher] {
        assert_eq!(
            fs::metadata(path).unwrap().modified().unwrap(),
            std::time::SystemTime::UNIX_EPOCH
        );
    }
}

#[test]
fn invalidates_theme_cache_when_repairing_an_icon() {
    let fixture = Fixture::new();
    let binary = Path::new("/tmp/virial-gpui");
    install(&fixture.0, binary).unwrap();
    let theme = fixture.0.join("icons/hicolor");
    let epoch = fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH);
    fs::File::open(&theme).unwrap().set_times(epoch).unwrap();
    assert!(!install(&fixture.0, binary).unwrap());
    assert_eq!(
        fs::metadata(&theme).unwrap().modified().unwrap(),
        std::time::SystemTime::UNIX_EPOCH
    );
    fs::write(theme.join("512x512/apps/virial-gpui.png"), b"broken").unwrap();
    assert!(install(&fixture.0, binary).unwrap());
    assert!(fs::metadata(&theme).unwrap().modified().unwrap() > std::time::SystemTime::UNIX_EPOCH);
}

#[test]
fn respects_absolute_xdg_paths_and_falls_back_for_empty_or_relative_values() {
    let home = PathBuf::from("/home/user");
    assert_eq!(
        data_home(Some("/custom/data".into()), Some(home.clone())).unwrap(),
        PathBuf::from("/custom/data")
    );
    for xdg in [None, Some(PathBuf::new()), Some("relative".into())] {
        assert_eq!(
            data_home(xdg, Some(home.clone())).unwrap(),
            home.join(".local/share")
        );
    }
    assert!(data_home(None, None).is_err());
}

#[test]
fn escapes_desktop_paths_without_injecting_keys_or_expanding_shell_characters() {
    assert_eq!(
        executable(Path::new("/tmp/a b\"$`\\%f\nvirial")).unwrap(),
        r#""/tmp/a b\\"\\$\\`\\\\%%f\nvirial""#
    );
    assert_eq!(
        value(Path::new("/tmp/icon\nHidden=true\t\\.png")).unwrap(),
        r"/tmp/icon\nHidden=true\t\\.png"
    );
}

#[test]
fn reports_registration_failure_without_removing_existing_files() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("icons"), "keep").unwrap();
    assert!(install(&fixture.0, Path::new("/tmp/virial-gpui")).is_err());
    assert_eq!(fs::read_to_string(fixture.0.join("icons")).unwrap(), "keep");
    assert!(!fixture.0.join("applications/virial-gpui.desktop").exists());
}
