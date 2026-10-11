//! PlantUML diagram-type recognition, mirroring upstream conventions.
//!
//! PlantUML does not publish one central detector like mermaid's
//! `detectType`: a block is typed by its `@start…` tag when it has a
//! dedicated one (`@startgantt`, `@startmindmap`, `@startwbs`, `@startjson`,
//! `@startyaml`, `@startditaa`, `@startsalt`), and otherwise by its first
//! significant keyword inside `@startuml` — the same table the official
//! documentation uses per diagram family (participant→sequence, usecase→
//! use-case, class/abstract→class, start/`:action`→activity, nwdiag→
//! network, archimate, salt, entity→ERD, concise/robust→timing, node/
//! artifact/database→deployment, component→component…).
//!
//! Known ambiguities, resolved like the PlantUML docs imply:
//! - `actor` is deliberately NOT mapped: PlantUML itself only decides
//!   between sequence and use-case from the arrows that follow.
//! - `interface` maps to class (the class-diagram page documents it as a
//!   class element); `entity` maps to ERD (the entity-relationship page's
//!   keyword) even though deployment also allows an `entity` element.
//! - A bare `@startuml` detects nothing by itself: the caller keeps reading
//!   following lines, because a more specific keyword usually follows.
//! - `@enduml` and common directives (`skinparam`, `title`, `hide`, `show`,
//!   `scale`, `theme`, `legend`, `left to right direction`) are not types.

/// Detect the PlantUML diagram family of a block, `None` when nothing
/// matches. Handles blocks with or without an `@start…` tag, and keeps
/// reading past a bare `@startuml` and its directives until a family
/// keyword shows up.
pub(crate) fn detect(source: &str) -> Option<&'static str> {
    source
        .lines()
        .map(str::trim_start)
        // Single-quote line comments are not keywords; `@enduml` closes.
        .filter(|line| !line.is_empty() && !line.starts_with('\''))
        .find_map(detect_line)
}

/// Classify one candidate line.
fn detect_line(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if trimmed.eq_ignore_ascii_case("@enduml") || trimmed.starts_with('\'') {
        return None;
    }
    // Dedicated start tags win outright.
    if let Some(rest) = trimmed.strip_prefix('@') {
        let (tag, _) = split_word(rest);
        return match tag.to_ascii_lowercase().as_str() {
            "startmindmap" => Some("mindmap"),
            "startwbs" => Some("wbs"),
            "startgantt" => Some("gantt"),
            "startjson" => Some("json-data"),
            "startyaml" => Some("yaml-data"),
            "startditaa" => Some("ditaa"),
            "startsalt" => Some("salt"),
            "startuml" | "start" => None,
            _ => None,
        };
    }
    keyword_family(trimmed)
}

/// First keyword of a line and the rest of it.
fn split_word(line: &str) -> (&str, &str) {
    let end = line
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    line.split_at(end)
}

/// The official per-family first keywords inside `@startuml`.
fn keyword_family(line: &str) -> Option<&'static str> {
    let (word, _) = split_word(line);
    // Activity-beta lines very often start with `:action;`.
    if word.starts_with(':') {
        return Some("activity");
    }
    match word {
        "participant" | "autonumber" | "activate" | "deactivate" | "destroy"
        | "box" => Some("sequence"),
        "usecase" => Some("use-case"),
        "class" | "abstract" | "interface" | "enum" => Some("class"),
        "object" => Some("object"),
        "state" => Some("state"),
        "component" => Some("component"),
        "node" | "artifact" | "agent" | "cloud" | "database" | "folder"
        | "frame" | "rectangle" | "storage" | "stack" | "card" | "file"
        | "boundary" | "control" | "queue" => Some("deployment"),
        "start" | "stop" | "fork" | "split" | "partition" | "repeat"
        | "detach" => Some("activity"),
        "nwdiag" => Some("network"),
        "archimate" => Some("archimate"),
        "salt" => Some("salt"),
        "entity" => Some("erd"),
        "concise" | "robust" | "binary" | "clock" => Some("timing"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(source: &str) -> &'static str {
        detect(source).expect("a PlantUML family")
    }

    #[test]
    fn dedicated_start_tags() {
        assert_eq!(kind("@startmindmap\n* root"), "mindmap");
        assert_eq!(kind("@startwbs\n* root"), "wbs");
        assert_eq!(kind("@startgantt\n[task] lasts 5 days"), "gantt");
        assert_eq!(kind("@startjson\n{\"a\": 1}"), "json-data");
        assert_eq!(kind("@startyaml\nkey: value"), "yaml-data");
        assert_eq!(kind("@startditaa\n+---+\n| A |\n+---+"), "ditaa");
        assert_eq!(kind("@startsalt\n{Login}"), "salt");
    }

    #[test]
    fn sequence_from_participant_and_friends() {
        assert_eq!(kind("@startuml\nparticipant A\nA -> B: hi"), "sequence");
        assert_eq!(kind("participant Bob"), "sequence");
        assert_eq!(kind("@startuml\nautonumber\nA -> B"), "sequence");
        assert_eq!(kind("@startuml\nactivate A"), "sequence");
    }

    #[test]
    fn class_family_and_its_elements() {
        assert_eq!(kind("@startuml\nclass Foo"), "class");
        assert_eq!(kind("@startuml\nabstract class Bar"), "class");
        assert_eq!(kind("@startuml\ninterface Baz"), "class");
        assert_eq!(kind("@startuml\nenum Color"), "class");
    }

    #[test]
    fn state_object_component_usecase() {
        assert_eq!(kind("@startuml\nstate s1"), "state");
        assert_eq!(kind("@startuml\nobject o1"), "object");
        assert_eq!(kind("@startuml\ncomponent web"), "component");
        assert_eq!(kind("@startuml\nusecase UC1"), "use-case");
    }

    #[test]
    fn deployment_elements() {
        assert_eq!(kind("@startuml\nnode server"), "deployment");
        assert_eq!(kind("@startuml\nartifact jar"), "deployment");
        assert_eq!(kind("@startuml\ndatabase db"), "deployment");
        assert_eq!(kind("@startuml\ncloud aws"), "deployment");
        assert_eq!(kind("@startuml\nframe bucket"), "deployment");
    }

    #[test]
    fn activity_beta_shapes() {
        assert_eq!(kind("@startuml\nstart\n:step;"), "activity");
        assert_eq!(kind("@startuml\n:step;\nstop"), "activity");
        assert_eq!(kind("@startuml\nfork\n:step;"), "activity");
        assert_eq!(kind("@startuml\nrepeat\n:step;"), "activity");
    }

    #[test]
    fn network_archimate_timing() {
        assert_eq!(kind("@startuml\nnwdiag { network a }"), "network");
        assert_eq!(kind("@startuml\narchimate #Technology srv"), "archimate");
        assert_eq!(kind("@startuml\nconcise A"), "timing");
        assert_eq!(kind("@startuml\nclock clk"), "timing");
    }

    #[test]
    fn erd_entity() {
        assert_eq!(kind("@startuml\nentity USER"), "erd");
    }

    #[test]
    fn actor_is_deliberately_untyped() {
        // Sequence or use-case: PlantUML needs the arrows to decide.
        assert!(detect("@startuml\nactor A").is_none());
    }

    #[test]
    fn bare_startuml_keeps_searching() {
        assert!(detect("@startuml").is_none());
        // Directives between the tag and the keyword do not hide it.
        assert_eq!(
            detect("@startuml\nskinparam shadowing false\nparticipant A"),
            Some("sequence")
        );
        assert!(detect("@startuml\nskinparam shadowing false").is_none());
    }

    #[test]
    fn enduml_and_comments_and_directives_are_not_types() {
        assert!(detect("@enduml").is_none());
        assert!(detect("' a comment").is_none());
        assert!(detect("@startuml\ntitle My title").is_none());
        assert!(detect("@startuml\nhide footbox").is_none());
        assert!(detect("@startuml\nscale 2").is_none());
    }

    #[test]
    fn no_type_in_plain_text() {
        assert!(detect("").is_none());
        assert!(detect("just some words").is_none());
    }
}
