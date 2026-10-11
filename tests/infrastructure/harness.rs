use super::*;

/// A temp home with a harness layout, passed explicitly to the seams under
/// test — no environment mutation, so the tests are parallel-safe.
fn with_home(setup: impl FnOnce(&Path), run: impl FnOnce(&Path)) {
    let dir = tempfile::tempdir().unwrap();
    setup(dir.path());
    run(dir.path());
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
            let info = inspect_with(&home.join(".claude"), home).unwrap();
            assert_eq!(info.name, "Claude Code");
            assert_eq!(info.role, Role::Home);
            assert_eq!(
                info.prompt.as_ref().map(|p| p.file_name().unwrap()),
                Some(std::ffi::OsStr::new("CLAUDE.md"))
            );
            assert!(info.credentials.iter().any(|name| name == "settings.json"));

            let agents = inspect_with(&home.join(".claude/agents"), home).unwrap();
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
            let codex = inspect_with(&home.join(".codex"), home).unwrap();
            assert_eq!(codex.name, "Codex");
            let hermes = inspect_with(&home.join(".hermes/profiles/explorateur"), home).unwrap();
            assert_eq!(hermes.name, "Hermes");
            assert_eq!(hermes.role, Role::Folder);
            let overmind = inspect_with(&home.join(".overmind"), home).unwrap();
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
                inspect_with(&home.join(".config/opencode"), home)
                    .unwrap()
                    .name,
                "OpenCode"
            );
            assert_eq!(
                inspect_with(&home.join(".zcode/cli"), home).unwrap().name,
                "Zcode"
            );
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
            assert_eq!(inspect_with(&home.join("Documents"), home), None);
            assert_eq!(inspect_with(&home.join(".claude-lookalike"), home), None);
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
            let found = installed_with(home);
            let names: Vec<_> = found.iter().map(|(name, _)| *name).collect();
            assert!(names.contains(&"Claude Code"));
            assert!(names.contains(&"Codex"));
            assert!(!names.contains(&"Hermes"), "absent homes are not listed");
        },
    );
}

#[test]
fn a_missing_home_reports_nothing_instead_of_guessing() {
    let missing = Path::new("Z:/virial-does-not-exist");
    assert_eq!(installed_with(missing), Vec::new());
    assert_eq!(inspect_with(missing, missing), None);
}

#[test]
fn the_audited_families_of_three_machines_are_recognized() {
    with_home(
        |home| {
            for dir in [
                ".cursor",
                ".cline",
                ".qoder",
                ".gemini",
                ".antigravity",
                ".blackboxcli",
                ".kilo",
                ".qwen",
                ".lark-cli",
                ".securecoder",
                ".moltbot",
                ".cagent",
                ".agents",
                ".hermes-dev",
                ".cargo",
                ".rustup",
                ".bun",
                ".npm",
                ".docker",
                ".vscode",
                ".pm2",
                ".expo",
                ".gradle",
                ".gunicorn",
                ".ssh",
                ".msf4",
                ".john",
                ".havoc",
                ".sliver",
                ".nxc",
                ".tradingview-pilot",
                ".com.moomoo.OpenD",
                ".com.futunn.FutuOpenD",
            ] {
                std::fs::create_dir_all(home.join(dir)).unwrap();
            }
        },
        |home| {
            assert_eq!(
                inspect_with(&home.join(".cursor"), home).unwrap().name,
                "Cursor"
            );
            assert_eq!(
                inspect_with(&home.join(".cargo"), home).unwrap().name,
                "Cargo"
            );
            assert_eq!(
                inspect_with(&home.join(".msf4"), home).unwrap().name,
                "Metasploit"
            );
            assert_eq!(
                inspect_with(&home.join(".tradingview-pilot"), home)
                    .unwrap()
                    .name,
                "TradingView Pilot"
            );
        },
    );
}

#[test]
fn kind_is_reported_per_family() {
    with_home(
        |home| {
            std::fs::create_dir_all(home.join(".claude")).unwrap();
            std::fs::create_dir_all(home.join(".cargo")).unwrap();
            std::fs::create_dir_all(home.join(".msf4")).unwrap();
            std::fs::create_dir_all(home.join(".tradingview-pilot")).unwrap();
            std::fs::create_dir_all(home.join(".overmind")).unwrap();
        },
        |home| {
            assert_eq!(
                inspect_with(&home.join(".claude"), home).unwrap().kind,
                Kind::Agentic
            );
            assert_eq!(
                inspect_with(&home.join(".cargo"), home).unwrap().kind,
                Kind::Development
            );
            assert_eq!(
                inspect_with(&home.join(".msf4"), home).unwrap().kind,
                Kind::Security
            );
            assert_eq!(
                inspect_with(&home.join(".tradingview-pilot"), home)
                    .unwrap()
                    .kind,
                Kind::Trading
            );
            assert_eq!(
                inspect_with(&home.join(".overmind"), home).unwrap().kind,
                Kind::Server
            );
        },
    );
}

#[test]
fn sliver_and_kilo_have_alternate_homes() {
    with_home(
        |home| {
            std::fs::create_dir_all(home.join(".sliver-client")).unwrap();
            std::fs::create_dir_all(home.join(".kilocode")).unwrap();
        },
        |home| {
            assert_eq!(
                inspect_with(&home.join(".sliver-client"), home)
                    .unwrap()
                    .name,
                "Sliver"
            );
            assert_eq!(
                inspect_with(&home.join(".kilocode"), home).unwrap().name,
                "Kilo Code"
            );
        },
    );
}

#[test]
fn security_and_trading_icons_are_embedded() {
    for name in ["shield", "chart", "tools"] {
        assert!(
            crate::ui::icons::icon_names().contains(&name),
            "icon {name} is not embedded"
        );
    }
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

#[test]
fn homes_are_joined_portably_across_separators() {
    // The AppData/Local/hermes home uses a nested relative path; joining must
    // work regardless of the platform separator.
    for harness in HARNESS_LIST {
        for home in harness.homes {
            assert!(!home.contains('\\'), "{}: {home}", harness.name);
        }
    }
}
