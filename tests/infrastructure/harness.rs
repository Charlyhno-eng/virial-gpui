use super::*;

fn with_home(setup: impl FnOnce(&Path), run: impl FnOnce(&Path)) {
    let dir = tempfile::tempdir().unwrap();
    // SAFETY of the env mutation is single-threaded test execution
    // (--test-threads=1 in CI and locally).
    std::env::set_var("VIRIAL_HOME_OVERRIDE", dir.path());
    setup(dir.path());
    run(dir.path());
    std::env::remove_var("VIRIAL_HOME_OVERRIDE");
}

#[test]
fn recognizes_a_claude_code_home_and_its_subfolders() {
    with_home(
        |home| {
            std::fs::create_dir_all(home.join(".claude/agents")).unwrap();
            std::fs::write(home.join(".claude/settings.json"), "{}").unwrap();
            std::fs::write(home.join(".claude/CLAUDE.md"), "# prompt").unwrap();
        },
        |home| {
            let info = inspect(&home.join(".claude")).unwrap();
            assert_eq!(info.name, "Claude Code");
            assert_eq!(info.role, Role::Home);
            assert_eq!(
                info.prompt.as_ref().map(|p| p.file_name().unwrap()),
                Some(std::ffi::OsStr::new("CLAUDE.md"))
            );
            assert!(info
                .credentials
                .iter()
                .any(|name| name == "settings.json"));

            let agents = inspect(&home.join(".claude/agents")).unwrap();
            assert_eq!(agents.name, "Claude Code");
            assert_eq!(agents.role, Role::Folder);
        },
    );
}

#[test]
fn recognizes_codex_hermes_and_overmind_layouts() {
    with_home(
        |home| {
            std::fs::create_dir_all(home.join(".codex/memories")).unwrap();
            std::fs::write(home.join(".codex/config.toml"), "").unwrap();
            std::fs::create_dir_all(home.join(".hermes/profiles/explorateur")).unwrap();
            std::fs::write(home.join(".hermes/SOUL.md"), "").unwrap();
            std::fs::create_dir_all(home.join(".overmind")).unwrap();
            std::fs::write(home.join(".overmind/.env"), "").unwrap();
        },
        |home| {
            let codex = inspect(&home.join(".codex")).unwrap();
            assert_eq!(codex.name, "Codex");
            let hermes = inspect(&home.join(".hermes/profiles/explorateur")).unwrap();
            assert_eq!(hermes.name, "Hermes");
            assert_eq!(hermes.role, Role::Folder);
            let overmind = inspect(&home.join(".overmind")).unwrap();
            assert_eq!(overmind.name, "Overmind");
            assert!(overmind.credentials.iter().any(|name| name == ".env"));
        },
    );
}

#[test]
fn opencode_and_zcode_nested_homes_are_found() {
    with_home(
        |home| {
            std::fs::create_dir_all(home.join(".config/opencode")).unwrap();
            std::fs::create_dir_all(home.join(".zcode/cli")).unwrap();
        },
        |home| {
            assert_eq!(
                inspect(&home.join(".config/opencode")).unwrap().name,
                "OpenCode"
            );
            assert_eq!(inspect(&home.join(".zcode/cli")).unwrap().name, "Zcode");
        },
    );
}

#[test]
fn a_foreign_folder_is_not_recognized() {
    with_home(
        |home| {
            std::fs::create_dir_all(home.join(".claude")).unwrap();
            std::fs::create_dir_all(home.join("Documents")).unwrap();
        },
        |home| {
            assert_eq!(inspect(&home.join("Documents")), None);
            assert_eq!(inspect(&home.join(".claude-lookalike")), None);
        },
    );
}

#[test]
fn installed_lists_every_harness_that_exists() {
    with_home(
        |home| {
            std::fs::create_dir_all(home.join(".claude")).unwrap();
            std::fs::create_dir_all(home.join(".codex")).unwrap();
        },
        |home| {
            let found = installed();
            let names: Vec<_> = found.iter().map(|(name, _)| *name).collect();
            assert!(names.contains(&"Claude Code"));
            assert!(names.contains(&"Codex"));
            assert!(!names.contains(&"Hermes"), "absent homes are not listed");
            let _ = home;
        },
    );
}

#[test]
fn a_missing_home_reports_nothing_instead_of_guessing() {
    std::env::set_var("VIRIAL_HOME_OVERRIDE", "Z:/virial-does-not-exist");
    assert_eq!(installed(), Vec::new());
    std::env::remove_var("VIRIAL_HOME_OVERRIDE");
}

#[test]
fn every_harness_uses_an_icon_that_ships_with_the_binary() {
    for harness in HARNESS_LIST {
        assert!(
            crate::ui::icons::icon_names().contains(&harness.icon),
            "{} uses unknown icon {}",
            harness.name,
            harness.icon
        );
    }
}
