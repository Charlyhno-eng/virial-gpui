//! Mermaid diagram-type recognition, mirroring upstream mermaid.js.
//!
//! The recognized kinds and their detection order mirror the official
//! `packages/mermaid/src/diagram-api/diagram-orchestration.ts` registration
//! list (mermaid `develop`, October 2026): the first detector that matches
//! wins, and `flowchart-elk`, `mindmap` and `architecture` are tested before
//! the plain flowchart exactly like upstream. The text is preprocessed the
//! same way too — YAML front matter skipped, `%%{...}%%` directives dropped,
//! `%%` comments stripped — so a block gets the same type here as in
//! mermaid-based tools (mermaid-cli, Kroki, VS Code and GitHub previews).
//!
//! Faithfulness notes: keyword detectors keep upstream's quirks (a leading
//! keyword is matched by prefix, so `pie` also claims a line starting with
//! `piechart`-like text; keywords are case-sensitive except where upstream
//! says otherwise). Divergences are deliberately limited to pathological
//! front-matter shapes where upstream trades quadratic backtracking for
//! subtlety; a first-closing-fence scan is enough for a preview header.

/// Every diagram type mermaid can detect, in display order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DiagramKind {
    FlowchartElk,
    Mindmap,
    Architecture,
    Agentflow,
    C4,
    Kanban,
    ClassDiagram,
    ErDiagram,
    Gantt,
    Info,
    Pie,
    Requirement,
    Sequence,
    Swimlane,
    Flowchart,
    Timeline,
    GitGraph,
    StateDiagram,
    Journey,
    QuadrantChart,
    Sankey,
    Packet,
    XyChart,
    Block,
    EventModeling,
    TreeView,
    Radar,
    Ishikawa,
    Treemap,
    Railroad,
    RailroadEbnf,
    RailroadAbnf,
    RailroadPeg,
    Venn,
    Wardley,
    Cynefin,
    Usecase,
    Error,
}

impl DiagramKind {
    /// The upstream diagram id, as mermaid itself names the type.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::FlowchartElk => "flowchart-elk",
            Self::Mindmap => "mindmap",
            Self::Architecture => "architecture",
            Self::Agentflow => "agentflow",
            Self::C4 => "c4",
            Self::Kanban => "kanban",
            Self::ClassDiagram => "classDiagram",
            Self::ErDiagram => "er",
            Self::Gantt => "gantt",
            Self::Info => "info",
            Self::Pie => "pie",
            Self::Requirement => "requirement",
            Self::Sequence => "sequence",
            Self::Swimlane => "swimlane",
            Self::Flowchart => "flowchart-v2",
            Self::Timeline => "timeline",
            Self::GitGraph => "gitGraph",
            Self::StateDiagram => "stateDiagram",
            Self::Journey => "journey",
            Self::QuadrantChart => "quadrantChart",
            Self::Sankey => "sankey",
            Self::Packet => "packet",
            Self::XyChart => "xychart",
            Self::Block => "block",
            Self::EventModeling => "eventmodeling",
            Self::TreeView => "treeView",
            Self::Radar => "radar",
            Self::Ishikawa => "ishikawa",
            Self::Treemap => "treemap",
            Self::Railroad => "railroad",
            Self::RailroadEbnf => "railroad-ebnf",
            Self::RailroadAbnf => "railroad-abnf",
            Self::RailroadPeg => "railroad-peg",
            Self::Venn => "venn",
            Self::Wardley => "wardley",
            Self::Cynefin => "cynefin",
            Self::Usecase => "usecase",
            Self::Error => "error",
        }
    }
}

/// Detect the diagram type of a mermaid block, `None` when nothing matches.
pub(crate) fn detect(source: &str) -> Option<DiagramKind> {
    let text = strip_front_matter(source);
    let text = strip_directives(text);
    let text = strip_comments(&text);
    // The `error` pseudo-diagram claims a body that is exactly that word.
    if text.trim().eq_ignore_ascii_case("error") {
        return Some(DiagramKind::Error);
    }
    // Upstream anchors each detector with `^\s*`, and `\s` spans newlines, so
    // the keyword sits at the start of the first non-blank line.
    let first = text
        .lines()
        .map(str::trim_start)
        .find(|line| !line.is_empty())?;
    detect_first_line(first)
}

/// Run the upstream detectors, in registration order, on the first keyword
/// line. Case-sensitive by prefix unless upstream says otherwise.
fn detect_first_line(first: &str) -> Option<DiagramKind> {
    // `keyword` + word boundary: the prefix matches and the next character is
    // not a word character (or there is none) — mirrors a trailing `\b`.
    let bounded = |prefix: &str| {
        first.starts_with(prefix)
            && first[prefix.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
    };
    // `keyword` followed by whitespace or end — mirrors `(?:\s|$)`.
    let spaced = |prefix: &str| {
        first.starts_with(prefix)
            && first[prefix.len()..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
    };
    // Case-insensitive prefix, no boundary — mirrors a trailing `/i`.
    let ci = |prefix: &str| {
        first
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    };
    let c4 = [
        "C4Context",
        "C4Container",
        "C4Component",
        "C4Dynamic",
        "C4Deployment",
    ]
    .iter()
    .any(|kind| first.starts_with(kind));

    Some(if first.starts_with("flowchart-elk") {
        DiagramKind::FlowchartElk
    } else if first.starts_with("mindmap") {
        DiagramKind::Mindmap
    } else if first.starts_with("architecture") {
        DiagramKind::Architecture
    } else if bounded("agentflow-beta") {
        DiagramKind::Agentflow
    } else if c4 {
        DiagramKind::C4
    } else if first.starts_with("kanban") {
        DiagramKind::Kanban
    } else if first.starts_with("classDiagram") {
        DiagramKind::ClassDiagram
    } else if first.starts_with("erDiagram") {
        DiagramKind::ErDiagram
    } else if first.starts_with("gantt") {
        DiagramKind::Gantt
    } else if first.starts_with("info") {
        DiagramKind::Info
    } else if first.starts_with("pie") {
        DiagramKind::Pie
    } else if first.starts_with("requirement") {
        DiagramKind::Requirement
    } else if first.starts_with("sequenceDiagram") {
        DiagramKind::Sequence
    } else if bounded("swimlane-beta") {
        DiagramKind::Swimlane
    } else if first.starts_with("graph") || first.starts_with("flowchart") {
        DiagramKind::Flowchart
    } else if first.starts_with("timeline") {
        DiagramKind::Timeline
    } else if first.starts_with("gitGraph") {
        DiagramKind::GitGraph
    } else if first.starts_with("stateDiagram") {
        DiagramKind::StateDiagram
    } else if first.starts_with("journey") {
        DiagramKind::Journey
    } else if first.starts_with("quadrantChart") {
        DiagramKind::QuadrantChart
    } else if first.starts_with("sankey") {
        DiagramKind::Sankey
    } else if first.starts_with("packet") {
        DiagramKind::Packet
    } else if first.starts_with("xychart") {
        DiagramKind::XyChart
    } else if first.starts_with("block") {
        DiagramKind::Block
    } else if first.starts_with("eventmodeling") {
        DiagramKind::EventModeling
    } else if first.starts_with("treeView-beta") {
        DiagramKind::TreeView
    } else if first.starts_with("radar-beta") {
        DiagramKind::Radar
    } else if ci("ishikawa-beta") || bounded_ci(first, "ishikawa") {
        DiagramKind::Ishikawa
    } else if first.starts_with("treemap") {
        DiagramKind::Treemap
    } else if ci("railroad-beta") {
        DiagramKind::Railroad
    } else if ci("railroad-ebnf-beta") {
        DiagramKind::RailroadEbnf
    } else if ci("railroad-abnf-beta") {
        DiagramKind::RailroadAbnf
    } else if ci("railroad-peg-beta") {
        DiagramKind::RailroadPeg
    } else if first.starts_with("venn-beta") {
        DiagramKind::Venn
    } else if ci("wardley-beta") {
        DiagramKind::Wardley
    } else if cynefin(first) {
        DiagramKind::Cynefin
    } else if spaced("usecase-beta") {
        DiagramKind::Usecase
    } else {
        return None;
    })
}

/// `ishikawa` or `ishikawa-beta`, case-insensitive, with a trailing `\b`.
fn bounded_ci(first: &str, prefix: &str) -> bool {
    first.len() >= prefix.len()
        && first[..prefix.len()].eq_ignore_ascii_case(prefix)
        && first[prefix.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
}

/// `cynefin-beta` followed by whitespace, `:` or end — mirrors `(?:[\s:]|$)`.
fn cynefin(first: &str) -> bool {
    const PREFIX: &str = "cynefin-beta";
    first.starts_with(PREFIX)
        && first[PREFIX.len()..]
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == ':')
}

/// Skip a Jekyll-style `---` YAML front matter block: optional same-line
/// indent, `---`, a body, and a closing `---` line with the same indent.
/// Without a closed fence the text is left alone, like upstream.
fn strip_front_matter(text: &str) -> &str {
    let indent_len = text.len() - text.trim_start_matches([' ', '\t']).len();
    let indent = &text[..indent_len];
    let rest = &text[indent_len..];
    let Some(body) = rest.strip_prefix("---") else {
        return text;
    };
    // The opening fence must end with a line break (trailing spaces allowed,
    // but the break itself is required: `---x` is just a paragraph).
    let after_open = body.trim_start_matches([' ', '\t']);
    let Some(after_open) = after_open.strip_prefix('\n') else {
        return text;
    };
    let closing = format!("{indent}---");
    // Offset of the first body line within `text`.
    let offset = text.len() - after_open.len();
    let mut consumed = 0; // bytes of `after_open` already scanned
    for line in after_open.split('\n') {
        if line.trim_end() == closing {
            // Everything after the closing fence line is the diagram text.
            return text.get(offset + consumed + line.len() + 1..).unwrap_or("");
        }
        consumed += line.len() + 1;
    }
    text
}

/// Drop `%%{...}%%` directives; an unterminated `%%{` is cut at its newline.
fn strip_directives(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("%%{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 3..];
        match after.find("}%%") {
            Some(close) => rest = &after[close + 3..],
            // Upstream's optional closing makes an unterminated directive a
            // dead prefix; cutting the line keeps the next keyword readable.
            None => {
                let end = after.find('\n').unwrap_or(after.len());
                rest = &after[end..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Strip `%%` comments: drop from the marker to the end of the line, keeping
/// the line break so blank-line structure survives.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        match line.find("%%") {
            Some(marker) => out.push_str(&line[..marker]),
            None => out.push_str(line),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(source: &str) -> &'static str {
        detect(source).expect("a diagram type").id()
    }

    fn none(source: &str) {
        assert!(detect(source).is_none(), "expected no diagram type");
    }

    #[test]
    fn flowcharts_in_both_spellings() {
        assert_eq!(kind("flowchart TD\n  a-->b"), "flowchart-v2");
        assert_eq!(kind("graph LR\n a-->b"), "flowchart-v2");
    }

    #[test]
    fn elk_wins_over_the_plain_flowchart() {
        assert_eq!(kind("flowchart-elk TD\n a-->b"), "flowchart-elk");
    }

    #[test]
    fn sequence_class_and_state_with_version_suffixes() {
        assert_eq!(kind("sequenceDiagram\n A->>B: hi"), "sequence");
        assert_eq!(kind("classDiagram\n class Foo"), "classDiagram");
        assert_eq!(kind("classDiagram-v2\n class Foo"), "classDiagram");
        assert_eq!(kind("stateDiagram-v2\n [*] --> s1"), "stateDiagram");
        assert_eq!(kind("stateDiagram\n [*] --> s1"), "stateDiagram");
    }

    #[test]
    fn er_gantt_pie_and_the_rest_of_the_classics() {
        assert_eq!(kind("erDiagram\n USER||--o{POST : writes"), "er");
        assert_eq!(kind("gantt\n title Plan"), "gantt");
        assert_eq!(kind("pie title Share\n \"a\": 45"), "pie");
        assert_eq!(kind("journey\n title Trip"), "journey");
        assert_eq!(kind("gitGraph\n commit"), "gitGraph");
        assert_eq!(kind("quadrantChart\n x-axis A"), "quadrantChart");
        assert_eq!(kind("requirementDiagram\n requirement r1"), "requirement");
        assert_eq!(kind("requirement\n requirement r1"), "requirement");
    }

    #[test]
    fn the_beta_generation() {
        assert_eq!(kind("sankey-beta\n\nA,B,1"), "sankey");
        assert_eq!(kind("xychart-beta\n x-axis [1, 2]"), "xychart");
        assert_eq!(kind("block-beta\n columns 1"), "block");
        assert_eq!(kind("packet-beta\n 0-15: id"), "packet");
        assert_eq!(kind("kanban\n todo[To do]"), "kanban");
        assert_eq!(kind("architecture-beta\n group x"), "architecture");
        assert_eq!(kind("radar-beta\n axis m"), "radar");
        assert_eq!(kind("treemap\n section A"), "treemap");
        assert_eq!(kind("venn-beta\n set A"), "venn");
        assert_eq!(kind("eventmodeling\n :step"), "eventmodeling");
        assert_eq!(kind("treeView-beta\n root"), "treeView");
    }

    #[test]
    fn the_newest_beta_generation() {
        assert_eq!(kind("agentflow-beta\n start"), "agentflow");
        assert_eq!(kind("swimlane-beta\n lane L"), "swimlane");
        assert_eq!(kind("ishikawa\n problem"), "ishikawa");
        assert_eq!(kind("ISHIKAWA-BETA\n problem"), "ishikawa");
        assert_eq!(kind("wardley-beta\n anchor"), "wardley");
        assert_eq!(kind("Wardley-Beta\n anchor"), "wardley");
        assert_eq!(kind("cynefin-beta\n clear"), "cynefin");
        assert_eq!(kind("railroad-beta\n alt"), "railroad");
        assert_eq!(kind("railroad-ebnf-beta\n rule"), "railroad-ebnf");
        assert_eq!(kind("railroad-abnf-beta\n rule"), "railroad-abnf");
        assert_eq!(kind("railroad-peg-beta\n rule"), "railroad-peg");
        assert_eq!(kind("usecase-beta\n actor"), "usecase");
    }

    #[test]
    fn boundary_rules_reject_lookalikes() {
        none("swimlane-beta2\n x");
        none("usecase-betamax\n x");
        none("cynefin-betax\n x");
    }

    #[test]
    fn c4_contexts_all_five_variants() {
        assert_eq!(kind("C4Context\n Person"), "c4");
        assert_eq!(kind("C4Container\n Container"), "c4");
        assert_eq!(kind("C4Component\n Component"), "c4");
        assert_eq!(kind("C4Dynamic\n Dynamic"), "c4");
        assert_eq!(kind("C4Deployment\n DeploymentNode"), "c4");
    }

    #[test]
    fn the_error_pseudo_diagram_is_exact_text() {
        assert_eq!(kind("error"), "error");
        assert_eq!(kind("  Error  "), "error");
        none("error\n more");
    }

    #[test]
    fn frontmatter_is_skipped() {
        let source = "---\ntitle: Plan\n---\ngantt\n title Plan";
        assert_eq!(kind(source), "gantt");
        // An unclosed fence is not front matter; upstream then finds no
        // diagram type at all — the leading `---` claims nothing.
        none("---\ntitle: x\ngantt");
    }

    #[test]
    fn directives_are_dropped() {
        assert_eq!(
            kind("%%{init: {\"theme\": \"dark\"}}%%\nsequenceDiagram\n A->>B: hi"),
            "sequence"
        );
    }

    #[test]
    fn comments_are_stripped() {
        assert_eq!(kind("%% a comment\ngraph TD\n a-->b"), "flowchart-v2");
        assert_eq!(kind("  %% indented comment\npie title P"), "pie");
    }

    #[test]
    fn leading_blank_lines_are_fine() {
        assert_eq!(kind("\n\n  \ngraph TD\n a-->b"), "flowchart-v2");
    }

    #[test]
    fn unknown_or_empty_blocks_recognize_nothing() {
        none("");
        none("   \n  \n");
        none("this is not a diagram");
        none("GRAPH TD\n a-->b");
        none("flow charts are nice");
    }

    #[test]
    fn keywords_are_prefix_matched_like_upstream() {
        // `info` by plain prefix, as upstream's `/^\s*info/`.
        assert_eq!(kind("information\n x"), "info");
        assert_eq!(kind("block-beta\n x"), "block");
        // `block` without the suffix also matches (`block(-beta)?`).
        assert_eq!(kind("block\n x"), "block");
    }
}
