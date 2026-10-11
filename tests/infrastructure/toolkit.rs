//! Tests for the sessions, skills and MCP listings.
//!
//! Every fixture mirrors a layout read off a real installation: Claude Code
//! keeps `sessions/<pid>.json` with a `cwd`/`startedAt` header and one
//! conversation per `projects/<encoded-cwd>/` folder; Codex writes
//! `sessions/<YYYY>/<MM>/<DD>/rollout-<iso>-<uuid>.jsonl`.

use super::*;
use std::fs;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("virial-toolkit-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_missing_home_lists_nothing() {
    let dir = scratch("absent");
    let toolkit = toolkit_with(&dir.join(".claude"));
    assert!(toolkit.sessions.is_empty());
    assert!(toolkit.skills.is_empty());
    assert!(toolkit.servers.is_empty());
}

#[test]
fn claude_sessions_are_read_with_their_working_directory() {
    let dir = scratch("claude-sessions");
    let home = dir.join(".claude");
    // The real header shape, including a Windows path with backslashes.
    write(
        &home.join("sessions").join("30868.json"),
        r#"{"cwd":"C:\\Users\\Deamon\\Desktop\\Backup\\Workflow","entrypoint":"cli","kind":"session","pid":30868,"sessionId":"2156fc63-413e","startedAt":"2026-10-11T09:12:00.000Z","version":"2.0.0"}"#,
    );
    let toolkit = toolkit_with(&home);
    assert_eq!(toolkit.sessions.len(), 1, "one session file");
    let session = &toolkit.sessions[0];
    assert_eq!(session.name, "30868.json");
    assert_eq!(
        session.cwd.as_deref(),
        Some(Path::new(r"C:\Users\Deamon\Desktop\Backup\Workflow"))
    );
    assert_eq!(session.started.as_deref(), Some("2026-10-11T09:12:00.000Z"));
}

#[test]
fn codex_sessions_are_found_through_their_date_tree() {
    let dir = scratch("codex-sessions");
    let home = dir.join(".codex");
    let name = "rollout-2026-05-10T12-30-19-019e12b9-af5e-7101-88ad-7b00ee76e21f.jsonl";
    write(
        &home
            .join("sessions")
            .join("2026")
            .join("05")
            .join("10")
            .join(name),
        "{}\n",
    );
    write(
        &home
            .join("sessions")
            .join("2026")
            .join("05")
            .join("11")
            .join("rollout-2026-05-11T09-00-00-11111111-2222-3333-4444-555555555555.jsonl"),
        "{}\n",
    );
    let toolkit = toolkit_with(&home);
    assert_eq!(toolkit.sessions.len(), 2, "both days are walked");
    assert!(
        toolkit
            .sessions
            .iter()
            .all(|s| s.name.starts_with("rollout-"))
    );
    // Non-jsonl files in the same tree are ignored.
    write(
        &home
            .join("sessions")
            .join("2026")
            .join("05")
            .join("10")
            .join("notes.txt"),
        "x",
    );
    assert_eq!(toolkit_with(&home).sessions.len(), 2);
}

#[test]
fn claude_project_conversations_are_listed_under_their_project() {
    let dir = scratch("claude-projects");
    let home = dir.join(".claude");
    let project = home.join("projects").join("C--Users-Deamon-Desktop-Backup");
    write(
        &project.join("9f2c1e33-aaaa-bbbb-cccc-000000000000.jsonl"),
        "{}\n",
    );
    write(&project.join("memory").join("MEMORY.md"), "# notes\n");
    let toolkit = toolkit_with(&home);
    assert_eq!(toolkit.sessions.len(), 1);
    assert_eq!(toolkit.sessions[0].cwd.as_deref(), Some(project.as_path()));
}

#[test]
fn a_skill_directory_reports_its_front_matter_description() {
    let dir = scratch("skills");
    let home = dir.join(".claude");
    write(
        &home.join("skills").join("web-search").join("SKILL.md"),
        "---\nname: web-search\ndescription: Search the web and cite sources\n---\n\n# Web search\n",
    );
    write(
        &home.join("skills").join("no-manifest").join("README.md"),
        "# not a skill\n",
    );
    let toolkit = toolkit_with(&home);
    assert_eq!(
        toolkit.skills.len(),
        1,
        "only the directory with SKILL.md counts"
    );
    let skill = &toolkit.skills[0];
    assert_eq!(skill.name, "web-search");
    assert_eq!(
        skill.description.as_deref(),
        Some("Search the web and cite sources")
    );
}

#[test]
fn a_skill_without_front_matter_falls_back_to_its_heading() {
    let dir = scratch("skill-heading");
    let home = dir.join(".claude");
    write(
        &home.join("skills").join("plain").join("SKILL.md"),
        "# Plain skill\n\nBody.\n",
    );
    let toolkit = toolkit_with(&home);
    assert_eq!(
        toolkit.skills[0].description.as_deref(),
        Some("Plain skill")
    );
}

#[test]
fn mcp_servers_are_named_with_their_transport() {
    let servers = servers_from_json(
        r#"{"mcpServers":{"gmail":{"command":"npx","args":["-y","@foo/gmail"]},"api":{"url":"https://example.test/mcp","type":"http"},"stream":{"url":"https://example.test/sse","type":"sse"}}}"#,
    );
    let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["gmail", "api", "stream"]);
    assert_eq!(servers[0].transport.as_deref(), Some("stdio"));
    assert_eq!(servers[1].transport.as_deref(), Some("http"));
    assert_eq!(servers[2].transport.as_deref(), Some("sse"));
}

#[test]
fn the_alternate_servers_spelling_is_also_read() {
    let servers = servers_from_json(r#"{"servers":{"one":{"command":"x"},"two":{"command":"y"}}}"#);
    let mut names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["one", "two"]);
}

#[test]
fn a_server_is_never_listed_twice() {
    let servers = servers_from_json(
        r#"{"mcpServers":{"dup":{"command":"x"}},"servers":{"dup":{"command":"y"}}}"#,
    );
    assert_eq!(servers.len(), 1, "the same name collapses to one entry");
}

#[test]
fn a_configuration_without_servers_yields_nothing() {
    assert!(servers_from_json(r#"{"model":"opus"}"#).is_empty());
    assert!(servers_from_json("").is_empty());
    assert!(servers_from_json("{ not json at all").is_empty());
}

#[test]
fn mcp_sources_lists_only_files_that_exist() {
    let dir = scratch("mcp-sources");
    write(&dir.join(".mcp.json"), "{}");
    let sources = mcp_sources(&dir);
    assert_eq!(sources.len(), 1);
    assert!(sources[0].ends_with(".mcp.json"));
}

#[test]
fn the_mcp_artwork_ships_with_the_binary() {
    assert!(
        crate::ui::icons::icon_names().contains(&MCP_ICON),
        "the MCP icon must be embedded"
    );
}

#[test]
fn an_unknown_application_still_lists_its_skills() {
    let dir = scratch("unknown");
    let home = dir.join(".zcode");
    write(
        &home.join("skills").join("alpha").join("SKILL.md"),
        "# Alpha\n",
    );
    let toolkit = toolkit_with(&home);
    assert_eq!(toolkit.skills.len(), 1);
    assert!(
        toolkit.sessions.is_empty(),
        "no known session layout to walk"
    );
}
