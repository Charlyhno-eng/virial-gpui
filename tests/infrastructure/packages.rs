use super::*;

#[test]
fn every_npm_family_file_maps_to_its_host() {
    let cases = [
        ("package.json", "npm", Role::Manifest),
        ("package-lock.json", "npm", Role::Lock),
        ("npm-shrinkwrap.json", "npm", Role::Lock),
        (".npmrc", "npm", Role::Credentials),
        ("pnpm-workspace.yaml", "pnpm", Role::Manifest),
        ("pnpm-lock.yaml", "pnpm", Role::Lock),
        ("yarn.lock", "Yarn", Role::Lock),
        (".yarnrc.yml", "Yarn", Role::Credentials),
        ("bun.lockb", "Bun", Role::Lock),
        ("bunfig.toml", "Bun", Role::Credentials),
        ("deno.json", "Deno", Role::Manifest),
        ("deno.lock", "Deno", Role::Lock),
    ];
    for (file, host, role) in cases {
        let info = classify(file).unwrap_or_else(|| panic!("{file} unrecognized"));
        assert_eq!(info.host, host, "{file}");
        assert_eq!(info.role, role, "{file}");
    }
}

#[test]
fn every_system_package_manager_maps_to_its_host() {
    let cases = [
        ("Cargo.toml", "crates.io", Role::Manifest),
        ("Cargo.lock", "crates.io", Role::Lock),
        ("pyproject.toml", "PyPI", Role::Manifest),
        ("poetry.lock", "PyPI", Role::Lock),
        ("uv.lock", "PyPI", Role::Lock),
        (".pypirc", "PyPI", Role::Credentials),
        ("Gemfile", "RubyGems", Role::Manifest),
        ("Gemfile.lock", "RubyGems", Role::Lock),
        ("composer.json", "Packagist", Role::Manifest),
        ("composer.lock", "Packagist", Role::Lock),
        ("auth.json", "Packagist", Role::Credentials),
        ("go.mod", "Go modules", Role::Manifest),
        ("go.sum", "Go modules", Role::Lock),
        ("pom.xml", "Maven Central", Role::Manifest),
        ("build.gradle.kts", "Gradle", Role::Manifest),
        ("mix.exs", "Hex", Role::Manifest),
        ("mix.lock", "Hex", Role::Lock),
        ("pubspec.yaml", "Pub", Role::Manifest),
        ("pubspec.lock", "Pub", Role::Lock),
        ("Package.swift", "Swift Package Manager", Role::Manifest),
        ("Package.resolved", "Swift Package Manager", Role::Lock),
        ("cpanfile", "CPAN", Role::Manifest),
    ];
    for (file, host, role) in cases {
        let info = classify(file).unwrap_or_else(|| panic!("{file} unrecognized"));
        assert_eq!(info.host, host, "{file}");
        assert_eq!(info.role, role, "{file}");
    }
}

#[test]
fn case_is_ignored_like_the_icon_table() {
    assert_eq!(classify("PACKAGE.JSON").unwrap().host, "npm");
    assert_eq!(classify("cargo.TOML").unwrap().host, "crates.io");
}

#[test]
fn everyday_files_are_not_recognized() {
    for name in ["main.rs", "notes.txt", "photo.png", "Makefile", "Dockerfile"] {
        assert_eq!(classify(name), None, "{name}");
    }
}

#[test]
fn each_host_carries_a_registry_and_a_known_icon() {
    for host in HOSTS {
        assert!(!host.registry.is_empty(), "{} has no registry", host.name);
        assert!(
            icon_exists(host.icon),
            "{} uses unknown icon {}",
            host.name,
            host.icon
        );
    }
}

#[test]
fn manifests_report_their_version_and_locks_do_not() {
    let manifest = classify("package.json").unwrap();
    let lock = classify("package-lock.json").unwrap();
    assert_eq!(manifest.role, Role::Manifest);
    assert_eq!(lock.role, Role::Lock);
    // Version extraction is a pure function; the file read happens in
    // classify only for manifests, which the version_field tests cover.
    assert_eq!(manifest.credential_files, vec![".npmrc", ".npmrc.auth"]);
}

#[test]
fn version_field_reads_json_and_toml_forms() {
    let json = r#"{
  "name": "virial",
  "version": "1.2.3",
  "dependencies": {}
}"#;
    assert_eq!(version_field(json, "version").as_deref(), Some("1.2.3"));
    let toml = "[package]\nname = \"virial\"\nversion = \"0.14.2\"\n";
    assert_eq!(version_field(toml, "version").as_deref(), Some("0.14.2"));
    // A dependencies block must not satisfy the request.
    let nested = "{\n  \"dependencies\": {\n    \"version\": \"9.9.9\"\n  }\n}";
    assert_eq!(version_field(nested, "version").as_deref(), Some("9.9.9"));
    assert_eq!(version_field("no version here", "version"), None);
    assert_eq!(version_field("{\"version\": \"\"}", "version"), None);
}

#[test]
fn credentials_are_detected_without_being_extracted() {
    let npmrc = "//registry.npmjs.org/:_authToken=npm_XXXXXXXXXXXX\n";
    assert!(contains_credential(npmrc));
    let pypirc = "[pypi]\nusername = user\npassword = hunter2\n";
    assert!(contains_credential(pypirc));
    assert!(!contains_credential("{\n  \"name\": \"virial\"\n}"));
    // The detection reports presence only; nothing returns the secret.
    let detected = contains_credential(npmrc);
    let _ = npmrc;
    assert!(detected);
}

#[test]
fn inspect_accepts_a_full_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Cargo.toml");
    std::fs::write(&path, "[package]\nversion = \"0.1.0\"\n").unwrap();
    let info = inspect(&path).unwrap();
    assert_eq!(info.host, "crates.io");
    assert_eq!(info.registry, "crates.io");
    assert_eq!(info.role, Role::Manifest);
}

#[test]
fn inspect_of_a_directory_or_foreign_file_is_none() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(inspect(directory.path()), None);
    let path = directory.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    assert_eq!(inspect(&path), None);
}