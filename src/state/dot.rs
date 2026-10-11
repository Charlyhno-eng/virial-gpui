//! Graphviz DOT recognition, mirroring the `dot` language guide's shapes.
//!
//! Graphviz has no runtime type detector — the engine (`dot`, `neato`,
//! `fdp`, `sfdp`, `twopi`, `circo`, `patchwork`, `osage`) is the layout, and
//! the diagram *shape* follows from the constructs used: `digraph` plus
//! `record`/`Mrecord` nodes is a data structure diagram, `subgraph cluster`
//! is a grouped flowchart, `rank=same` and `->` chains are the classic
//! flowchart, a single node with `shape=plaintext` and an HTML-like label is
//! a table, and undirected `--` graphs read as mind-map-like association
//! diagrams. This module classifies the source shape, the way the Graphviz
//! gallery organizes its examples.
//!
//! Engine keywords (`neato`, `circo`…) are recognized when they appear as
//! the graph kind prefix — the `-K` engine override convention — but the
//! engine is a layout, not a diagram type; it is surfaced as the caption.

/// Detect the Graphviz diagram shape of a block, `None` when nothing
/// matches.
pub(crate) fn detect(source: &str) -> Option<&'static str> {
    let mut digraph = false;
    let mut graph = false;
    let mut clusters = 0usize;
    let mut records = false;
    let mut html_label = false;
    let mut engine = None;
    for line in source.lines() {
        let line = strip_comment(line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if engine.is_none()
            && let Some((kind, _)) = split_word(trimmed)
            && matches!(
                kind,
                "neato" | "fdp" | "sfdp" | "twopi" | "circo" | "patchwork" | "osage"
            )
        {
            engine = Some(kind);
        }
        if trimmed.contains("digraph") {
            digraph = true;
        }
        if trimmed.contains("graph") && !trimmed.contains("digraph") {
            graph = true;
        }
        if trimmed.contains("subgraph") && trimmed.contains("cluster") {
            clusters += 1;
        }
        if trimmed.contains("record") {
            records = true;
        }
        if trimmed.contains("<") && trimmed.contains(">") && trimmed.contains("label") {
            html_label = true;
        }
    }
    if !digraph && !graph {
        // The `-K` engine alone is still a DOT program worth naming.
        return engine;
    }
    let kind = if digraph {
        if records {
            "record-data-structure"
        } else if clusters > 0 {
            "clustered-flowchart"
        } else {
            "directed-graph"
        }
    } else if html_label {
        "table"
    } else {
        "undirected-graph"
    };
    Some(kind)
}

/// Strip `//` and `#` line comments (DOT also allows `/* */`, which this
/// line-oriented scan intentionally ignores).
fn strip_comment(line: &str) -> &str {
    match (line.find("//"), line.find('#')) {
        (Some(a), Some(b)) => &line[..a.min(b)],
        (Some(a), None) => &line[..a],
        (None, Some(b)) => &line[..b],
        (None, None) => line,
    }
}

/// First word of a line and the rest of it.
fn split_word(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    let end = trimmed
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, _)| i)?;
    Some(trimmed.split_at(end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(source: &str) -> &'static str {
        detect(source).expect("a Graphviz shape")
    }

    #[test]
    fn classic_digraph() {
        assert_eq!(kind("digraph G { a -> b; }"), "directed-graph");
    }

    #[test]
    fn record_nodes_are_data_structures() {
        assert_eq!(
            kind("digraph G { node [shape=record]; a [label=\"{1|2}\"]; }"),
            "record-data-structure"
        );
        assert_eq!(
            kind("digraph G { a [shape=Mrecord, label=\"{f1|f2}\"]; }"),
            "record-data-structure"
        );
    }

    #[test]
    fn clusters_are_grouped_flowcharts() {
        assert_eq!(
            kind("digraph G { subgraph cluster_0 { a -> b; } c -> d; }"),
            "clustered-flowchart"
        );
    }

    #[test]
    fn undirected_graphs() {
        assert_eq!(kind("graph G { a -- b; b -- c; }"), "undirected-graph");
    }

    #[test]
    fn an_html_label_table() {
        assert_eq!(
            kind("graph G { a [label=<<table><tr><td>x</td></tr></table>>]; }"),
            "table"
        );
    }

    #[test]
    fn engine_prefixes_are_recognized_alone() {
        assert_eq!(kind("neato { a -- b; }"), "neato");
        assert_eq!(kind("circo { a -- b; }"), "circo");
    }

    #[test]
    fn comments_and_blank_lines_do_not_break_detection() {
        assert_eq!(
            kind("// a comment\ndigraph G {\n # another\n a -> b;\n}"),
            "directed-graph"
        );
    }

    #[test]
    fn no_shape_without_a_graph() {
        assert!(detect("").is_none());
        assert!(detect("just text").is_none());
        assert!(detect("{ a; b; }").is_none());
    }
}
