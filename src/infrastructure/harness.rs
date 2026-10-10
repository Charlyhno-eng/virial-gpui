//! Harness awareness for the file explorer: where the major agentic CLIs keep
//! their configuration, prompts, sessions and memories, so a user browsing
//! their home directory sees which harness owns which folder.
//!
//! Layouts follow what the harnesses actually write and what the local
//! Overmind workflow uses:
//!
//! - Claude Code    ~/.claude/                    settings.json, agents/, skills/
//! - Codex          ~/.codex/                     config.toml, auth.json, sessions/
//! - Hermes         ~/.hermes/  (Windows:         profiles/<name>/{config.yaml,
//!                  %LOCALAPPDATA%\hermes)        SOUL.md, .env, memories/, skills/)
//! - OpenClaw       ~/.config/openclaw/           config, agents/
//! - Zcode          ~/.zcode/                     cli/, workspace/, v2/
//! - OpenCode       ~/.config/opencode/           config, agents/
//! - Overmind       ~/.overmind/                  .env, .mcp.json, hermes/ shared home

use std::path::{Path, PathBuf};

/// One harness: its display name, its artwork in the icon table, and the
/// folders it owns under the user's home. The first existing folder wins.
pub struct Harness {
    pub name: &'static str,
    pub icon: &'static str,
    /// Folder names relative to the home directory, in preference order. The
    /// first one that exists identifies the installation.
    pub homes: &'static [&'static str],
    /// Files and folders that belong to this harness, matched by name inside
    /// one of its homes (`""` means the home folder itself).
    pub markers: &'static [&'static str],
    /// Where the harness keeps its system prompt, when it has one.
    pub prompt: &'static [&'static str],
    /// Where per-harness credentials live, listed for the details panel.
    pub credentials: &'static [&'static str],
}

pub const HARNESS_LIST: &[Harness] = &[
    Harness {
        name: "Claude Code",
        icon: "config",
        homes: &[".claude"],
        markers: &["", "agents", "skills", "commands", "settings.json", "projects", "sessions"],
        prompt: &["CLAUDE.md", "agents"],
        credentials: &[".credentials.json", "settings.json"],
    },
    Harness {
        name: "Codex",
        icon: "config",
        homes: &[".codex"],
        markers: &["", "config.toml", "auth.json", "sessions", "memories", "logs_2.sqlite"],
        prompt: &["AGENTS.md", "instructions.md"],
        credentials: &["auth.json", ".env"],
    },
    Harness {
        name: "Hermes",
        icon: "config",
        homes: &[".hermes", "AppData/Local/hermes"],
        markers: &["", "profiles", "skills", "memories", "sessions", "cron", "plugins"],
        prompt: &["SOUL.md", "profiles"],
        credentials: &[".env", "profiles"],
    },
    Harness {
        name: "OpenClaw",
        icon: "config",
        homes: &[".config/openclaw", ".openclaw"],
        markers: &["", "agents", "config", "workspace"],
        prompt: &["SOUL.md", "agents"],
        credentials: &[".env", "auth.json"],
    },
    Harness {
        name: "Zcode",
        icon: "config",
        homes: &[".zcode"],
        markers: &["", "cli", "workspace", "v2", "plugin-workspace"],
        prompt: &["SOUL.md", "workspace"],
        credentials: &[".env"],
    },
    Harness {
        name: "OpenCode",
        icon: "config",
        homes: &[".config/opencode", ".opencode"],
        markers: &["", "agents", "config.json", "opencode.json"],
        prompt: &["AGENTS.md", "agents"],
        credentials: &[".env", "auth.json"],
    },
    Harness {
        name: "Overmind",
        icon: "config",
        homes: &[".overmind"],
        markers: &["", ".env", ".mcp.json", "hermes"],
        prompt: &["hermes"],
        credentials: &[".env", ".env.postgres"],
    },
];

/// What the explorer shows for a folder that belongs to a harness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessInfo {
    pub name: &'static str,
    pub icon: &'static str,
    /// The home folder that actually exists on this machine.
    pub home: PathBuf,
    /// What the inspected folder is within the harness layout.
    pub role: Role,
    /// The system-prompt file this folder provides, when found.
    pub prompt: Option<PathBuf>,
    /// Credential files present in the inspected folder, by name only.
    pub credentials: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The harness home itself.
    Home,
    /// A subfolder the harness owns (agents, skills, memories…).
    Folder,
}

/// The user home directory, portable across Windows and Unix. Overridable for
/// tests through `VIRIAL_HOME_OVERRIDE`.
fn home() -> Option<PathBuf> {
    if let Ok(override_home) = std::env::var("VIRIAL_HOME_OVERRIDE") {
        return Some(PathBuf::from(override_home));
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("LOCALAPPDATA").ok().map(PathBuf::from))
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// Recognize a folder as belonging to a harness, reading only what it needs:
/// the folder's own name and, for the match, one directory listing per
/// candidate home.
pub fn inspect(path: &Path) -> Option<HarnessInfo> {
    let home_root = home()?;
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    for harness in HARNESS_LIST {
        for relative in harness.homes {
            let candidate = home_root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
            let Ok(candidate) = candidate.canonicalize() else {
                continue;
            };
            if absolute == candidate {
                return Some(finish(harness, candidate, Role::Home));
            }
            if absolute.starts_with(&candidate) {
                return Some(finish(harness, candidate, Role::Folder));
            }
        }
    }
    None
}

/// Gather the per-folder facts once the harness and its home are known.
fn finish(harness: &Harness, home: PathBuf, role: Role) -> HarnessInfo {
    let prompt = harness
        .prompt
        .iter()
        .map(|name| home.join(name))
        .find(|candidate| candidate.exists());
    let credentials = harness
        .credentials
        .iter()
        .map(|name| home.join(name))
        .filter(|candidate| candidate.exists())
        .filter_map(|candidate| candidate.file_name().map(|name| name.to_string_lossy().into_owned()))
        .collect();
    HarnessInfo {
        name: harness.name,
        icon: harness.icon,
        home,
        role,
        prompt,
        credentials,
    }
}

/// Every harness installed on this machine, for an overview entry point.
pub fn installed() -> Vec<(&'static str, PathBuf)> {
    let Some(home_root) = home() else {
        return Vec::new();
    };
    HARNESS_LIST
        .iter()
        .filter_map(|harness| {
            harness
                .homes
                .iter()
                .map(|relative| home_root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR)))
                .find(|candidate| candidate.is_dir())
                .map(|home| (harness.name, home))
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/infrastructure/harness.rs"]
mod tests;