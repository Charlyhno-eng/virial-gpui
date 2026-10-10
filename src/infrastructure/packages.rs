//! Package-registry awareness for the file explorer, in the spirit of VS
//! Code's language/manifest detection: recognize the manifest, lock and
//! credential files of the major package hosts, show their artwork, and read
//! the versions they pin.
//!
//! Everything is local: the module reads files it is given, it never opens
//! network connections, and it treats credential files as opaque (it reports
//! that a key exists, never its value).

use std::path::Path;

/// A package host: where its packages live, which files describe a project,
/// and which file holds its credentials.
pub struct Host {
    pub name: &'static str,
    /// Lower-case artwork name under `assets/icons/`.
    pub icon: &'static str,
    /// Where packages are published from, e.g. `registry.npmjs.org`.
    pub registry: &'static str,
    /// Exact file names that mark a project of this host.
    pub manifests: &'static [&'static str],
    /// Lock files pinning resolved versions.
    pub locks: &'static [&'static str],
    /// Files that carry authentication tokens or API keys for this host.
    pub credentials: &'static [&'static str],
    /// The JSON/TOML field a manifest stores its own version under.
    pub version_field: Option<&'static str>,
}

/// Every host the explorer recognizes. The order is the lookup order, so the
/// more specific hosts come first (pnpm and Yarn before npm's shared `.npmrc`).
pub const HOSTS: &[Host] = &[
    Host {
        name: "npm",
        icon: "javascript",
        registry: "registry.npmjs.org",
        manifests: &["package.json"],
        locks: &["package-lock.json", "npm-shrinkwrap.json"],
        credentials: &[".npmrc", ".npmrc.auth"],
        version_field: Some("version"),
    },
    Host {
        name: "pnpm",
        icon: "config",
        registry: "registry.npmjs.org",
        manifests: &["pnpm-workspace.yaml"],
        locks: &["pnpm-lock.yaml"],
        credentials: &[".npmrc"],
        version_field: Some("version"),
    },
    Host {
        name: "Yarn",
        icon: "config",
        registry: "registry.yarnpkg.com",
        manifests: &[],
        locks: &["yarn.lock"],
        credentials: &[".yarnrc.yml", ".yarnrc"],
        version_field: Some("version"),
    },
    Host {
        name: "Bun",
        icon: "config",
        registry: "registry.npmjs.org",
        manifests: &[],
        locks: &["bun.lockb", "bun.lock"],
        credentials: &["bunfig.toml"],
        version_field: Some("version"),
    },
    Host {
        name: "Deno",
        icon: "config",
        registry: "jsr.io",
        manifests: &["deno.json", "deno.jsonc"],
        locks: &["deno.lock"],
        credentials: &[],
        version_field: Some("version"),
    },
    Host {
        name: "crates.io",
        icon: "rust",
        registry: "crates.io",
        manifests: &["Cargo.toml"],
        locks: &["Cargo.lock"],
        credentials: &["credentials.toml", ".cargo/config.toml"],
        version_field: Some("version"),
    },
    Host {
        name: "PyPI",
        icon: "python",
        registry: "pypi.org",
        manifests: &["pyproject.toml", "setup.py", "setup.cfg"],
        locks: &["poetry.lock", "uv.lock", "requirements.txt", "Pipfile.lock"],
        credentials: &[".pypirc"],
        version_field: Some("version"),
    },
    Host {
        name: "RubyGems",
        icon: "code",
        registry: "rubygems.org",
        manifests: &["Gemfile", "mygem.gemspec"],
        locks: &["Gemfile.lock"],
        credentials: &["credentials"],
        version_field: Some("version"),
    },
    Host {
        name: "Packagist",
        icon: "config",
        registry: "packagist.org",
        manifests: &["composer.json"],
        locks: &["composer.lock"],
        credentials: &["auth.json"],
        version_field: Some("version"),
    },
    Host {
        name: "NuGet",
        icon: "code",
        registry: "api.nuget.org",
        manifests: &["nuget.config", "packages.config"],
        locks: &["packages.lock.json", "project.assets.json"],
        credentials: &["nuget.config"],
        version_field: Some("Version"),
    },
    Host {
        name: "Go modules",
        icon: "code",
        registry: "proxy.golang.org",
        manifests: &["go.mod"],
        locks: &["go.sum"],
        credentials: &[".netrc"],
        version_field: None,
    },
    Host {
        name: "Maven Central",
        icon: "code",
        registry: "repo.maven.apache.org",
        manifests: &["pom.xml"],
        locks: &[],
        credentials: &["settings.xml"],
        version_field: Some("version"),
    },
    Host {
        name: "Gradle",
        icon: "code",
        registry: "repo.maven.apache.org",
        manifests: &["build.gradle", "build.gradle.kts", "settings.gradle"],
        locks: &["gradle.lockfile"],
        credentials: &["gradle.properties", "gradle.properties.auth"],
        version_field: Some("version"),
    },
    Host {
        name: "Hex",
        icon: "code",
        registry: "hex.pm",
        manifests: &["mix.exs"],
        locks: &["mix.lock"],
        credentials: &[],
        version_field: None,
    },
    Host {
        name: "Pub",
        icon: "code",
        registry: "pub.dev",
        manifests: &["pubspec.yaml"],
        locks: &["pubspec.lock"],
        credentials: &[],
        version_field: Some("version"),
    },
    Host {
        name: "Swift Package Manager",
        icon: "code",
        registry: "swiftpm",
        manifests: &["Package.swift"],
        locks: &["Package.resolved"],
        credentials: &[],
        version_field: None,
    },
    Host {
        name: "Hackage",
        icon: "code",
        registry: "hackage.haskell.org",
        manifests: &[],
        locks: &["cabal.project.freeze"],
        credentials: &[".cabal/config"],
        version_field: None,
    },
    Host {
        name: "CPAN",
        icon: "code",
        registry: "cpan.org",
        manifests: &["cpanfile", "Makefile.PL", "Build.PL"],
        locks: &["META.json"],
        credentials: &[],
        version_field: None,
    },
];

/// What the explorer shows for one file that belongs to a package host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageInfo {
    pub host: &'static str,
    pub icon: &'static str,
    pub registry: &'static str,
    /// What this specific file is within the host's layout.
    pub role: Role,
    /// The project version declared in this file, when it carries one.
    pub version: Option<String>,
    /// The credential files this host uses, listed for the details panel.
    pub credential_files: Vec<&'static str>,
    /// True when this exact file carries host credentials (a key exists).
    pub holds_credentials: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Manifest,
    Lock,
    Credentials,
}

/// Recognize a file name as belonging to a host. Names are matched exactly and
/// case-insensitively, like the file-type icon table.
pub fn classify(file_name: &str) -> Option<PackageInfo> {
    let lowered = file_name.to_ascii_lowercase();
    for host in HOSTS {
        let fields = |names: &[&'static str]| {
            names
                .iter()
                .any(|candidate| candidate.to_ascii_lowercase() == lowered)
        };
        let role = if fields(host.manifests) {
            Some(Role::Manifest)
        } else if fields(host.locks) {
            Some(Role::Lock)
        } else if fields(host.credentials) {
            Some(Role::Credentials)
        } else {
            None
        };
        if let Some(role) = role {
            return Some(PackageInfo {
                host: host.name,
                icon: host.icon,
                registry: host.registry,
                role,
                version: None,
                credential_files: host.credentials.to_vec(),
                holds_credentials: role == Role::Credentials,
            });
        }
    }
    None
}

/// The value of a `"version"`-style field in JSON or TOML text, taken from the
/// first match. Handles both quote styles and trailing commas; anything the
/// parser cannot confirm reads as absent rather than guessed.
pub fn version_field(text: &str, field: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        let pattern = format!("\"{field}\"");
        let candidate = if let Some(rest) = trimmed.strip_prefix(&pattern) {
            rest.trim_start().strip_prefix(':')
        } else {
            trimmed
                .strip_prefix(field)
                .and_then(|rest| rest.trim_start().strip_prefix('='))
        };
        if let Some(rest) = candidate {
            let value = rest.trim().trim_end_matches(',').trim();
            let value = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(value);
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

/// True when the text contains a credential-looking assignment for the host's
/// known key names (an `_authToken`, an `api_key`, a token…). Only presence is
/// reported; the value is never extracted.
pub fn contains_credential(text: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "_authToken",
        "_auth=",
        "api_key",
        "apiKey",
        "api-key",
        "token =",
        "token:",
        "password",
    ];
    NEEDLES.iter().any(|needle| text.contains(needle))
}

/// Artwork for a host, falling back to the generic document when the host's
/// icon is missing from the asset table.
#[cfg_attr(not(test), allow(dead_code))]
pub fn icon_exists(name: &str) -> bool {
    crate::ui::icons::icon_names()
        .iter()
        .any(|candidate| *candidate == name)
}

/// The `PackageInfo` for a directory entry path, or None when the file is not
/// recognized. The file is read once, for the version of a manifest or the
/// credential-presence flag of a credentials file.
pub fn inspect(path: &Path) -> Option<PackageInfo> {
    let name = path.file_name()?.to_str()?;
    let mut info = classify(name)?;
    let text = std::fs::read_to_string(path).ok();
    if let Some(text) = text.as_deref() {
        if info.role == Role::Manifest {
            if let Some(host) = HOSTS.iter().find(|host| host.name == info.host)
                && let Some(field) = host.version_field
            {
                info.version = version_field(text, field);
            }
        } else if info.holds_credentials {
            info.holds_credentials = contains_credential(text);
        }
    }
    Some(info)
}

#[cfg(test)]
#[path = "../../tests/infrastructure/packages.rs"]
mod tests;