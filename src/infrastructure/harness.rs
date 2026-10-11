//! Home-folder awareness for the file explorer: where the major agentic CLIs,
//! toolchains, security suites and trading servers keep their configuration,
//! prompts, sessions and memories, so a user browsing their home directory
//! sees which application owns which folder.
//!
//! The catalog comes from an audit of three real machines: a Windows
//! workstation, a Kali box and an Ubuntu MCP server. Layouts follow what the
//! applications actually write, and the local Overmind workflow's conventions.
//!
//! - Claude Code    ~/.claude/                    settings.json, agents/, skills/
//! - Codex          ~/.codex/                     config.toml, auth.json, sessions/
//! - Hermes         ~/.hermes/  (Windows:         profiles/<name>/{config.yaml,
//!   %LOCALAPPDATA%\hermes)                        SOUL.md, .env, memories/, skills/)
//! - OpenClaw       ~/.config/openclaw/           config, agents/
//! - Zcode          ~/.zcode/                     cli/, workspace/, v2/
//! - OpenCode       ~/.config/opencode/           config, agents/
//! - Overmind       ~/.overmind/                  .env, .mcp.json, hermes/ shared home
//! - Metasploit     ~/.msf4/  · John  ~/.john/  · Sliver ~/.sliver/ · NetExec ~/.nxc/

use std::path::{Path, PathBuf};

/// The broad family an application belongs to, shown in the details panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Agentic CLI harnesses: Claude Code, Codex, Hermes, Cursor…
    Agentic,
    /// Toolchains and editors: Cargo, Bun, npm, Docker, VS Code…
    Development,
    /// Offensive suites: Metasploit, John the Ripper, Havoc, Sliver…
    Security,
    /// Market-data gateways: TradingView Pilot, Moomoo/Futu OpenD.
    Trading,
    /// Server-side runtime: overmind, bridges, gunicorn…
    Server,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Agentic => "Agentic harness",
            Self::Development => "Development tool",
            Self::Security => "Security suite",
            Self::Trading => "Trading gateway",
            Self::Server => "Server runtime",
        }
    }
}

/// One application: its display name, its artwork in the icon table, and the
/// folders it owns under the user's home. The first existing folder wins.
pub struct Harness {
    pub name: &'static str,
    pub icon: &'static str,
    pub kind: Kind,
    /// Folder names relative to the home directory, in preference order. The
    /// first one that exists identifies the installation.
    pub homes: &'static [&'static str],
    /// Where the application keeps its system prompt, when it has one.
    pub prompt: &'static [&'static str],
    /// Where per-application credentials live, listed for the details panel.
    pub credentials: &'static [&'static str],
}

pub const HARNESS_LIST: &[Harness] = &[
    // ── Agentic harnesses ────────────────────────────────────────────────────
    Harness {
        name: "Claude Code",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".claude"],
        prompt: &["CLAUDE.md", "agents"],
        credentials: &[".credentials.json", "settings.json"],
    },
    Harness {
        name: "Codex",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".codex"],
        prompt: &["AGENTS.md", "instructions.md"],
        credentials: &["auth.json", ".env"],
    },
    Harness {
        name: "Hermes",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".hermes", "AppData/Local/hermes"],
        prompt: &["SOUL.md", "profiles"],
        credentials: &[".env", "profiles"],
    },
    Harness {
        name: "Hermes (dev)",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".hermes-dev"],
        prompt: &["SOUL.md", "profiles"],
        credentials: &[".env"],
    },
    Harness {
        name: "OpenClaw",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".config/openclaw", ".openclaw"],
        prompt: &["SOUL.md", "agents"],
        credentials: &[".env", "auth.json"],
    },
    Harness {
        name: "Zcode",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".zcode"],
        prompt: &["SOUL.md", "workspace"],
        credentials: &[".env"],
    },
    Harness {
        name: "OpenCode",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".config/opencode", ".opencode"],
        prompt: &["AGENTS.md", "agents"],
        credentials: &[".env", "auth.json"],
    },
    Harness {
        name: "Cursor",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".cursor"],
        prompt: &["AGENTS.md"],
        credentials: &[".env"],
    },
    Harness {
        name: "Cursor Server",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".cursor-server"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "Cline",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".cline"],
        prompt: &["AGENTS.md"],
        credentials: &[".env"],
    },
    Harness {
        name: "Qoder",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".qoder"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "Gemini CLI",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".gemini"],
        prompt: &[],
        credentials: &["oauth_creds.json", ".env"],
    },
    Harness {
        name: "Antigravity",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".antigravity", ".antigravity-ide"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "Blackbox CLI",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".blackboxcli"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "Kilo Code",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".kilo", ".kilocode"],
        prompt: &["AGENTS.md"],
        credentials: &[".env"],
    },
    Harness {
        name: "Qwen CLI",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".qwen"],
        prompt: &[],
        credentials: &[".env", "auth.json"],
    },
    Harness {
        name: "Lark CLI",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".lark-cli"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "SecureCoder",
        icon: "shield",
        kind: Kind::Agentic,
        homes: &[".securecoder"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "Moltbot",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".moltbot"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "Cagent",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".cagent"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "Agents",
        icon: "config",
        kind: Kind::Agentic,
        homes: &[".agents"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "Overmind",
        icon: "config",
        kind: Kind::Server,
        homes: &[".overmind"],
        prompt: &["hermes"],
        credentials: &[".env", ".env.postgres"],
    },
    // ── Development toolchains ──────────────────────────────────────────────
    Harness {
        name: "Cargo",
        icon: "rust",
        kind: Kind::Development,
        homes: &[".cargo"],
        prompt: &[],
        credentials: &["credentials.toml"],
    },
    Harness {
        name: "Rustup",
        icon: "rust",
        kind: Kind::Development,
        homes: &[".rustup"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "Bun",
        icon: "javascript",
        kind: Kind::Development,
        homes: &[".bun"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "npm",
        icon: "javascript",
        kind: Kind::Development,
        homes: &[".npm"],
        prompt: &[],
        credentials: &[".npmrc"],
    },
    Harness {
        name: "Docker",
        icon: "docker",
        kind: Kind::Development,
        homes: &[".docker"],
        prompt: &[],
        credentials: &["config.json"],
    },
    Harness {
        name: "VS Code",
        icon: "code",
        kind: Kind::Development,
        homes: &[".vscode"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "PM2",
        icon: "config",
        kind: Kind::Server,
        homes: &[".pm2"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "Expo",
        icon: "react",
        kind: Kind::Development,
        homes: &[".expo"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "Gradle",
        icon: "config",
        kind: Kind::Development,
        homes: &[".gradle"],
        prompt: &[],
        credentials: &["gradle.properties"],
    },
    Harness {
        name: "Gunicorn",
        icon: "config",
        kind: Kind::Server,
        homes: &[".gunicorn"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "SSH",
        icon: "network",
        kind: Kind::Development,
        homes: &[".ssh"],
        prompt: &[],
        credentials: &["config", "known_hosts"],
    },
    // ── Security suites (Kali arsenal) ──────────────────────────────────────
    Harness {
        name: "Metasploit",
        icon: "shield",
        kind: Kind::Security,
        homes: &[".msf4"],
        prompt: &[],
        credentials: &["database.yml", "config"],
    },
    Harness {
        name: "John the Ripper",
        icon: "shield",
        kind: Kind::Security,
        homes: &[".john"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "Havoc",
        icon: "shield",
        kind: Kind::Security,
        homes: &[".havoc"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "Sliver",
        icon: "shield",
        kind: Kind::Security,
        homes: &[".sliver", ".sliver-client"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "NetExec",
        icon: "shield",
        kind: Kind::Security,
        homes: &[".nxc"],
        prompt: &[],
        credentials: &[],
    },
    // ── Trading gateways ────────────────────────────────────────────────────
    Harness {
        name: "TradingView Pilot",
        icon: "chart",
        kind: Kind::Trading,
        homes: &[".tradingview-pilot"],
        prompt: &[],
        credentials: &[".env"],
    },
    Harness {
        name: "Moomoo OpenD",
        icon: "chart",
        kind: Kind::Trading,
        homes: &[".com.moomoo.OpenD"],
        prompt: &[],
        credentials: &[],
    },
    Harness {
        name: "Futu OpenD",
        icon: "chart",
        kind: Kind::Trading,
        homes: &[".com.futunn.FutuOpenD"],
        prompt: &[],
        credentials: &[],
    },
];

/// What the explorer shows for a folder that belongs to an application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessInfo {
    pub name: &'static str,
    pub icon: &'static str,
    pub kind: Kind,
    /// The home folder that actually exists on this machine.
    pub home: PathBuf,
    /// What the inspected folder is within the application layout.
    pub role: Role,
    /// The system-prompt file this folder provides, when found.
    pub prompt: Option<PathBuf>,
    /// Credential files present in the inspected folder, by name only.
    pub credentials: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The application home itself.
    Home,
    /// A subfolder the application owns (agents, skills, memories…).
    Folder,
}

/// The user home directory, portable across Windows and Unix.
fn home() -> Option<PathBuf> {
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

/// Recognize a folder as belonging to an application, reading only what it
/// needs: the folder's own name and, for the match, one directory listing per
/// candidate home.
pub fn inspect(path: &Path) -> Option<HarnessInfo> {
    let home_root = home()?;
    inspect_with(path, &home_root)
}

/// Test seam: the same recognition with an explicit home root.
pub fn inspect_with(path: &Path, home_root: &Path) -> Option<HarnessInfo> {
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

/// Gather the per-folder facts once the application and its home are known.
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
        .filter_map(|candidate| {
            candidate
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    HarnessInfo {
        name: harness.name,
        icon: harness.icon,
        kind: harness.kind,
        home,
        role,
        prompt,
        credentials,
    }
}

/// Every application installed on this machine, for an overview entry point.
/// (The UI entry point arrives with the overview; the seam stays for tests.)
#[allow(dead_code)]
pub fn installed() -> Vec<(&'static str, PathBuf)> {
    let Some(home_root) = home() else {
        return Vec::new();
    };
    installed_with(&home_root)
}

/// Test seam: the same listing with an explicit home root.
pub fn installed_with(home_root: &Path) -> Vec<(&'static str, PathBuf)> {
    HARNESS_LIST
        .iter()
        .filter_map(|harness| {
            harness
                .homes
                .iter()
                .map(|relative| {
                    home_root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR))
                })
                .find(|candidate| candidate.is_dir())
                .map(|home| (harness.name, home))
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/infrastructure/harness.rs"]
mod tests;
