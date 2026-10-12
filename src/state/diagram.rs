//! Unified diagram-family recognition for fenced code blocks.
//!
//! Three ecosystems dominate diagram-as-code in real repositories:
//! - mermaid (GitHub, GitLab, many docs sites) — 34 upstream types;
//! - PlantUML (UML-heavy projects, Confluence) — tagged `@start…` blocks;
//! - Graphviz DOT (academic and infrastructure docs) — `digraph`/`graph`.
//!
//! This module picks the right recognizer for a fenced block. Mermaid keeps
//! its upstream-faithful detector; PlantUML and DOT use the structural
//! conventions of their own documentation (see `plantuml.rs` and `dot.rs`).

/// The caption for a fenced block: `tool · type`, e.g. `mermaid · sequence`.
pub(crate) struct Caption {
    pub(crate) text: String,
}

/// Caption a fenced block from its info string and body, `None` when the
/// block is not a recognized diagram.
pub(crate) fn caption(language: &str, body: &str) -> Option<Caption> {
    let lower = language.to_ascii_lowercase();
    match lower.as_str() {
        "mermaid" => {
            crate::state::mermaid::detect(body).map(|kind| format!("mermaid · {}", kind.id()))
        }
        "plantuml" | "uml" | "puml" => {
            crate::state::plantuml::detect(body).map(|kind| format!("plantuml · {kind}"))
        }
        "dot" | "graphviz" | "gv" => {
            crate::state::dot::detect(body).map(|kind| format!("graphviz · {kind}"))
        }
        _ => None,
    }
    .map(|text| Caption { text })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_by_info_string() {
        assert_eq!(
            caption("mermaid", "sequenceDiagram\n A->>B").unwrap().text,
            "mermaid · sequence"
        );
        assert_eq!(
            caption("plantuml", "@startuml\nparticipant A")
                .unwrap()
                .text,
            "plantuml · sequence"
        );
        assert_eq!(
            caption("PUML", "@startmindmap\n* a").unwrap().text,
            "plantuml · mindmap"
        );
        assert_eq!(
            caption("dot", "digraph G { a -> b; }").unwrap().text,
            "graphviz · directed-graph"
        );
        assert_eq!(
            caption("Graphviz", "graph G { a -- b; }").unwrap().text,
            "graphviz · undirected-graph"
        );
    }

    #[test]
    fn non_diagram_languages_get_nothing() {
        assert!(caption("rust", "fn main() {}").is_none());
        assert!(caption("", "digraph G { a -> b; }").is_none());
        assert!(caption("python", "@startuml\nparticipant A").is_none());
    }

    #[test]
    fn recognized_language_with_unrecognized_body_gets_nothing() {
        assert!(caption("mermaid", "not a diagram").is_none());
        assert!(caption("plantuml", "@startuml\njust words").is_none());
        assert!(caption("dot", "just words").is_none());
    }
}
