//! Sessions, skills and MCP servers stored by the agentic harnesses.
//!
//! The catalog in [`super::harness`] says *which* folder belongs to which
//! application. This module answers the next three questions the details panel
//! asks once that is known: what conversations are stored there, what skills the
//! application can load, and which MCP servers it is wired to.
//!
//! Every layout below was read off a real installation rather than guessed:
//! Claude Code keeps `sessions/<pid>.json` (cwd, sessionId, startedAt) and
//! `projects/<encoded-cwd>/…`; Codex writes
//! `sessions/<YYYY>/<MM>/<DD>/rollout-<iso8601>-<uuid>.jsonl` whose first line is
//! a `session_meta` record carrying `cwd`, `id` and `timestamp`.
//!
//! Nothing here reads a file's contents to build a session list: names, dates
//! and counts come from paths and directory listings, and the single JSON line
//! read is the session header. Credential files are still listed by name only.

use std::path::{Path, PathBuf};

/// Artwork for the MCP row, so a server list reads as its own thing.
pub const MCP_ICON: &str = "mcp";

/// What a session list entry can tell us without opening the conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfo {
    /// File name of the session record, for display.
    pub name: String,
    /// Where the record lives.
    pub path: PathBuf,
    /// Working directory the session was started in, when the format records it.
    pub cwd: Option<PathBuf>,
    /// Start time, when the format records one.
    pub started: Option<String>,
}

/// A skill directory an application can load on demand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillInfo {
    pub name: String,
    pub path: PathBuf,
    /// `SKILL.md` carries a one-line description worth showing.
    pub description: Option<String>,
}

/// One MCP server the application is configured to reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    /// `stdio`, `http` or `sse` when the configuration states it.
    pub transport: Option<String>,
}

/// Everything the panel shows for one application home.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Toolkit {
    pub sessions: Vec<SessionInfo>,
    pub skills: Vec<SkillInfo>,
    pub servers: Vec<McpServer>,
}

// ── Sessions ─────────────────────────────────────────────────────────────────

/// Claude Code: `sessions/<pid>.json`, whose header names the working directory
/// and the session id. The file name is the pid, so the start time is only
/// available from the header.
fn claude_sessions(home: &Path, out: &mut Vec<SessionInfo>) {
    let dir = home.join("sessions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let header = read_json_header(&path);
        out.push(SessionInfo {
            cwd: header.get("cwd").map(PathBuf::from),
            started: header.get("startedAt").map(|s| s.to_string()),
            name,
            path,
        });
    }
}

/// Codex: `sessions/<YYYY>/<MM>/<DD>/rollout-<timestamp>-<uuid>.jsonl`. The
/// timestamp is in the file name, and the first line is a `session_meta` record.
fn codex_sessions(home: &Path, out: &mut Vec<SessionInfo>) {
    // Three levels: sessions/<YYYY>/<MM>/<DD>/rollout-<iso8601>-<uuid>.jsonl
    collect_codex_rollouts(&home.join("sessions"), 0, out);
}

/// Walk the date tree without assuming how many levels a build has produced:
/// a session file is a `.jsonl`, and everything between it and `sessions` is a
/// directory, so the recursion stops wherever the files are.
fn collect_codex_rollouts(dir: &Path, depth: usize, out: &mut Vec<SessionInfo>) {
    const MAX_DEPTH: usize = 3;
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_codex_rollouts(&path, depth + 1, out);
            continue;
        }
        if path
            .extension()
            .is_none_or(|extension| extension != "jsonl")
        {
            continue;
        }
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        // rollout-2026-05-10T12-30-19-<uuid>.jsonl
        let started = name
            .strip_prefix("rollout-")
            .and_then(|rest| rest.split_once('-').map(|(_, tail)| tail.to_string()));
        out.push(SessionInfo {
            cwd: None,
            started,
            name,
            path,
        });
    }
}

/// One chat per project directory: `projects/<encoded-cwd>/` holds the working
/// directory it belongs to and a `memory` folder. Claude Code stores the real
/// conversations as jsonl files inside those project folders.
fn claude_projects(home: &Path, out: &mut Vec<SessionInfo>) {
    let root = home.join("projects");
    let Ok(projects) = std::fs::read_dir(&root) else {
        return;
    };
    for project in projects.flatten() {
        let dir = project.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let started = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());
            out.push(SessionInfo {
                cwd: Some(dir.clone()),
                started,
                name,
                path,
            });
        }
    }
}

// ── Skills ───────────────────────────────────────────────────────────────────

/// A skill is a directory holding a `SKILL.md`; the first `# ` line under the
/// front matter is its description. Only that line is read.
fn collect_skills(root: &Path, out: &mut Vec<SkillInfo>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // A skill is a directory holding a SKILL.md; anything else in the tree
        // is layout, not a skill, and must not be listed as one.
        let manifest = path.join("SKILL.md");
        if !path.is_dir() || !manifest.is_file() {
            continue;
        }
        let description = std::fs::read_to_string(&manifest)
            .ok()
            .and_then(|text| skill_description(&text));
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        out.push(SkillInfo {
            name,
            path,
            description,
        });
    }
}

/// Read the `description:` field of the YAML front matter, falling back to the
/// first markdown heading. Neither is required.
fn skill_description(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("description:") {
            let value = rest.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    text.lines()
        .find_map(|line| line.strip_prefix("# ").map(|h| h.trim().to_string()))
        .filter(|h| !h.is_empty())
}

// ── MCP ──────────────────────────────────────────────────────────────────────

/// MCP servers are named in a JSON object under `mcpServers`. Both spellings
/// that the tools actually write are accepted: `mcpServers` in Claude Code's
/// settings and `servers` in several other harnesses.
///
/// The scan is a brace matcher, not a JSON parser: it tracks string state so a
/// `}` inside a command line or an argument cannot end the object early, and it
/// only ever extracts a key and a transport. Names are taken from the keys of
/// the object, never from array elements, so a server named `args` cannot be
/// mistaken for a server.
pub fn servers_from_json(text: &str) -> Vec<McpServer> {
    let mut found: Vec<McpServer> = Vec::new();
    for key in ["\"mcpServers\"", "\"servers\""] {
        for object in objects_after(text, key) {
            for (name, body) in top_level_keys(&object) {
                let transport = if body.contains("\"http\"") || body.contains("streamable-http") {
                    Some("http".to_string())
                } else if body.contains("\"sse\"") {
                    Some("sse".to_string())
                } else if body.contains("\"command\"") || body.contains("\"url\"") {
                    Some("stdio".to_string())
                } else {
                    None
                };
                if !found.iter().any(|s| s.name == name) {
                    found.push(McpServer { name, transport });
                }
            }
        }
    }
    found
}

/// The object literal that follows a key such as `"mcpServers"`.
fn objects_after(text: &str, key: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(key) {
        let start = from + at + key.len();
        let Some(open) = text[start..].find('{') else {
            break;
        };
        let open = start + open;
        if let Some(close) = matching_brace(text, open) {
            objects.push(text[open + 1..close].to_string());
        }
        from = open + 1;
    }
    objects
}

/// Index of the `}` that closes the `{` at `open`, skipping braces inside
/// strings and honouring backslash escapes.
fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if in_string => escaped = true,
            b'"' => in_string = !in_string,
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The immediate keys of a JSON object body, each with the text of its value.
///
/// Only keys at the object's own depth are returned, so keys inside a nested
/// object (`env`, `headers`) or an array (`args`) never become server names.
fn top_level_keys(body: &str) -> Vec<(String, String)> {
    let bytes = body.as_bytes();
    let mut keys: Vec<(String, String)> = Vec::new();
    // Walk the body as `key : value , key : value`.
    let mut cursor = 0usize;
    // Find the next quoted token that is followed by a colon.
    while let Some(open) = find_from(body, cursor, b'"') {
        let Some(close) = skip_string(bytes, open) else {
            break;
        };
        let name = body[open + 1..close - 1].to_string();
        let rest = &body[close..];
        let trimmed = rest.trim_start();
        let colon_at = close + (rest.len() - trimmed.len());
        if !trimmed.starts_with(':') {
            cursor = close;
            continue;
        }
        let value_start = colon_at + 1;
        let value_end = skip_value(bytes, value_start);
        keys.push((name, body[value_start..value_end].trim().to_string()));
        cursor = value_end;
    }
    keys
}

/// Index just past the closing quote of the string starting at `open`.
fn skip_string(bytes: &[u8], open: usize) -> Option<usize> {
    let mut index = open + 1;
    let mut escaped = false;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if !escaped => escaped = true,
            b'"' if !escaped => return Some(index + 1),
            _ => escaped = false,
        }
        index += 1;
    }
    None
}

/// Index just past the value that starts at `from`: a string, an object, an
/// array, or a bare literal up to the next comma at this depth.
fn skip_value(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\n' | b'\r' | b'\t') {
        index += 1;
    }
    if index >= bytes.len() {
        return index;
    }
    match bytes[index] {
        b'"' => skip_string(bytes, index).unwrap_or(bytes.len()),
        b'{' | b'[' => {
            let mut depth = 0i32;
            let mut cursor = index;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'"' => {
                        cursor = skip_string(bytes, cursor).unwrap_or(bytes.len());
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return cursor + 1;
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            bytes.len()
        }
        _ => {
            let mut cursor = index;
            while cursor < bytes.len() && bytes[cursor] != b',' {
                cursor += 1;
            }
            cursor
        }
    }
}

/// Offset of the next `needle` at or after `from`.
fn find_from(text: &str, from: usize, needle: u8) -> Option<usize> {
    text.as_bytes()[from..]
        .iter()
        .position(|byte| *byte == needle)
        .map(|at| from + at)
}

/// Configuration files that may list MCP servers, relative to an application
/// home. Only names are reported; the panel never shows a command line or a
/// URL with a token in it.
pub fn mcp_sources(home: &Path) -> Vec<PathBuf> {
    [
        ".mcp.json",
        "mcp.json",
        "settings.json",
        "config.json",
        "claude_desktop_config.json",
    ]
    .iter()
    .map(|name| home.join(name))
    .filter(|path| path.is_file())
    .collect()
}

// ── Entry points ─────────────────────────────────────────────────────────────

/// Everything one application home stores, read with a single listing per
/// known subfolder. Pure std: no dependency walks the tree.
pub fn toolkit_with(home: &Path) -> Toolkit {
    let mut toolkit = Toolkit::default();

    let has_claude = home.file_name().is_some_and(|n| n == ".claude");
    if has_claude {
        claude_sessions(home, &mut toolkit.sessions);
        claude_projects(home, &mut toolkit.sessions);
        collect_skills(&home.join("skills"), &mut toolkit.skills);
    }
    let has_codex = home.file_name().is_some_and(|n| n == ".codex");
    if has_codex {
        codex_sessions(home, &mut toolkit.sessions);
        collect_skills(&home.join("skills"), &mut toolkit.skills);
    }
    if !has_claude && !has_codex {
        collect_skills(&home.join("skills"), &mut toolkit.skills);
        collect_skills(&home.join("skill"), &mut toolkit.skills);
    }

    for source in mcp_sources(home) {
        let Ok(text) = std::fs::read_to_string(&source) else {
            continue;
        };
        for server in servers_from_json(&text) {
            if !toolkit.servers.iter().any(|s| s.name == server.name) {
                toolkit.servers.push(server);
            }
        }
    }

    toolkit.sessions.sort_by(|a, b| b.name.cmp(&a.name));
    toolkit
}

/// Read a flat JSON object's top-level string fields, without a JSON parser:
/// enough for the session headers, and it cannot execute anything.
fn read_json_header(path: &Path) -> std::collections::HashMap<String, String> {
    let mut fields = std::collections::HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return fields;
    };
    for key in ["cwd", "startedAt", "sessionId", "name"] {
        let needle = format!("\"{key}\"");
        let Some(start) = text.find(&needle) else {
            continue;
        };
        let rest = &text[start + needle.len()..];
        let Some(colon) = rest.find(':') else {
            continue;
        };
        let value = rest[colon + 1..].trim_start();
        let Some(value) = value.strip_prefix('"') else {
            continue;
        };
        // A Windows path is stored escaped (`C:\\Users`), so read the closing
        // quote with the escape state and decode: the panel shows the real path.
        let bytes = value.as_bytes();
        let mut escaped = false;
        let mut end = None;
        for (index, byte) in bytes.iter().enumerate() {
            if escaped {
                escaped = false;
                continue;
            }
            match byte {
                b'\\' => escaped = true,
                b'"' => {
                    end = Some(index);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else { continue };
        fields.insert(key.to_string(), unescape(&value[..end]));
    }
    fields
}

/// Decode the escapes JSON allows in a string, so an escaped Windows path reads
/// back as the path a user would recognise.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(decoded) => out.push(decoded),
                    None => out.push('\u{fffd}'),
                }
            }
            Some(other) => {
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
#[path = "../../tests/infrastructure/toolkit.rs"]
mod tests;
