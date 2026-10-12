//! Lightweight Markdown renderer that turns a document into styled GPUI lines.
//!
//! No dependency and no HTML pass: block structure is detected line by line,
//! and inline styling (bold, italic, code, links, strikethrough) is produced
//! as byte-range `HighlightStyle` spans — the same mechanism the syntax
//! colored code preview uses. This keeps the feature inside the app's own
//! theme and avoids pulling a markdown crate into the binary.
//!
//! Deliberate limits: no nested lists, no tables, no images, no raw HTML and
//! no multi-line quotes. They render as ordinary paragraphs — plain but
//! correct, never a crash.

use gpui::{HighlightStyle, SharedString, rgb, rgba};
use std::ops::Range;

use crate::ui::theme::ACCENT_BLUE;

/// Inline code: the syntect theme's string color on a faint surface tint.
fn code_span() -> HighlightStyle {
    HighlightStyle {
        color: Some(rgb(0x5cf3ff).into()),
        background_color: Some(rgba(0x35314355).into()),
        ..Default::default()
    }
}

/// Links: the application accent with the conventional underline.
fn link_span() -> HighlightStyle {
    HighlightStyle {
        color: Some(rgb(ACCENT_BLUE).into()),
        underline: Some(gpui::UnderlineStyle::default()),
        ..Default::default()
    }
}

fn emph_span(bold: bool) -> HighlightStyle {
    HighlightStyle {
        font_weight: bold.then_some(gpui::FontWeight::BOLD),
        font_style: (!bold).then_some(gpui::FontStyle::Italic),
        ..Default::default()
    }
}

fn del_span() -> HighlightStyle {
    HighlightStyle {
        strikethrough: Some(gpui::StrikethroughStyle::default()),
        ..Default::default()
    }
}

/// Block-level appearance of a rendered line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LineKind {
    Body,
    /// `#` heading, level 1..=6.
    Heading(u8),
    /// `>` quote line: italic, indented.
    Quote,
    /// Line inside a fenced code block.
    CodeBlock,
    /// First line of a fenced mermaid block: a small type caption.
    Mermaid,
    /// `---` thematic break.
    Rule,
}

/// One rendered line: text, left indent, block style and inline spans.
pub(crate) struct MarkdownLine {
    pub(crate) text: SharedString,
    /// Spaces of left padding, applied by the renderer.
    pub(crate) indent: usize,
    pub(crate) kind: LineKind,
    pub(crate) highlights: Vec<(Range<usize>, HighlightStyle)>,
}

fn plain(text: impl Into<SharedString>, kind: LineKind, indent: usize) -> MarkdownLine {
    MarkdownLine {
        text: text.into(),
        indent,
        kind,
        highlights: Vec::new(),
    }
}

/// Caption state for a fenced block being parsed.
#[derive(PartialEq)]
enum MermaidCaption {
    /// Not a diagram block (wrong info string or caption already settled).
    None,
    /// A diagram info string, waiting for the first body line to detect the
    /// diagram type.
    Pending,
    /// The type was already found; no more detection work.
    Done,
    /// First body line(s) held no keyword yet — remember how many body lines
    /// were pushed so far, so the caption can be inserted above the first
    /// one once a later line reveals the type (front matter, directives and
    /// comments can delay the keyword).
    Skip(usize),
}

/// Info strings of the diagram ecosystems this preview captions. Mermaid is
/// exact; PlantUML travels under several aliases and Graphviz under three.
fn diagram_language(language: &str) -> bool {
    matches!(
        language,
        "mermaid" | "plantuml" | "uml" | "puml" | "dot" | "graphviz" | "gv"
    )
}

/// The info string of an opening fence: the text after the three markers.
/// ` ```rust ` yields `rust`; ` ``` ` yields an empty string.
fn fenced_language(trimmed: &str, marker: char) -> Option<&str> {
    let rest = trimmed
        .strip_prefix(&marker.to_string().repeat(3))?
        .trim_end();
    // A longer fence like ` ```` ` still opens with the same marker; a fourth
    // backtick belongs to the fence, not the info string.
    let rest = rest.trim_start_matches(marker);
    let language = rest.trim();
    // CommonMark: an info string containing a backtick closes nothing here —
    // treat it as unknown so the caption stays off.
    if language.contains('`') || language.contains('~') {
        return None;
    }
    Some(language)
}

/// Is this line a thematic break? `---`, `***`, `___`, also with spaces.
fn is_rule(trimmed: &str) -> bool {
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() < 3 {
        return false;
    }
    let marker = chars[0];
    if !matches!(marker, '-' | '*' | '_') {
        return false;
    }
    chars.iter().all(|c| *c == marker || *c == ' ')
}

/// List marker at the start of a trimmed line: `- `, `* `, `+ `, `12. `.
/// Returns (marker byte length in the source, marker text as rendered).
fn list_marker(trimmed: &str) -> Option<(usize, String)> {
    let bytes = trimmed.as_bytes();
    match bytes.first()? {
        b'-' | b'*' | b'+' => {
            if bytes.get(1) == Some(&b' ') {
                Some((2, format!("{} ", &trimmed[..1])))
            } else {
                None
            }
        }
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if bytes.get(digits) == Some(&b'.') && bytes.get(digits + 1) == Some(&b' ') {
                let number = &trimmed[..digits];
                Some((digits + 2, format!("{number}. ")))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Parse a whole document into display lines.
pub(crate) fn parse(source: &str) -> Vec<MarkdownLine> {
    let source = source.replace("\r\n", "\n");
    let mut out = Vec::new();
    // Opening fence character ('`' or '~'); None when outside a fenced block.
    let mut fence: Option<char> = None;
    // Caption tracking for the open fenced block: `mermaid` blocks get a
    // type caption above their body. Detection is incremental — the first
    // non-blank body line usually decides, and if a front-matter block or
    // comments precede the keyword, the caption is inserted retroactively
    // above the first body line once a later line reveals the type.
    let mut mermaid = MermaidCaption::None;
    // Info string of the open diagram block (kept across body lines).
    let mut pending_language = String::new();
    let mut blank_streak = 0;

    for raw in source.split('\n') {
        let line = raw.trim_end();
        let trimmed = line.trim_start();

        if let Some(marker) = fence {
            // A closing fence is the marker repeated 3+ times, alone.
            let closing = !trimmed.is_empty()
                && trimmed.chars().all(|c| c == marker)
                && trimmed.chars().count() >= 3;
            if closing {
                fence = None;
                mermaid = MermaidCaption::None;
            } else {
                // A diagram caption is decided by the block's first
                // non-blank line — mermaid's own detectors only read that
                // line once front matter, directives and comments are
                // stripped, so feeding the accumulated body reproduces
                // upstream exactly while staying incremental.
                if mermaid == MermaidCaption::Pending && !trimmed.is_empty() {
                    if let Some(text) = crate::state::diagram::caption(&pending_language, trimmed) {
                        out.push(plain(text.text, LineKind::Mermaid, 0));
                        mermaid = MermaidCaption::Done;
                    } else {
                        mermaid = MermaidCaption::Skip(out.len());
                    }
                } else if let MermaidCaption::Skip(start) = mermaid
                    && !trimmed.is_empty()
                    && let Some(text) = crate::state::diagram::caption(&pending_language, trimmed)
                {
                    out.insert(start, plain(text.text, LineKind::Mermaid, 0));
                    mermaid = MermaidCaption::Done;
                }
                out.push(plain(line.to_owned(), LineKind::CodeBlock, 0));
            }
            continue;
        }

        if trimmed.len() >= 3 && trimmed.starts_with("```") {
            fence = Some('`');
            // A diagram block earns a type caption above its body; the type
            // is known once the first body line is seen.
            if let Some(language) = fenced_language(trimmed, '`')
                && diagram_language(language)
            {
                pending_language = language.to_owned();
                mermaid = MermaidCaption::Pending;
            }
            continue;
        }
        if trimmed.len() >= 3 && trimmed.starts_with("~~~") {
            fence = Some('~');
            if let Some(language) = fenced_language(trimmed, '~')
                && diagram_language(language)
            {
                pending_language = language.to_owned();
                mermaid = MermaidCaption::Pending;
            }
            continue;
        }

        // Collapse consecutive blank lines into a single spacer.
        if trimmed.is_empty() {
            blank_streak += 1;
            if blank_streak == 1 {
                out.push(plain("", LineKind::Body, 0));
            }
            continue;
        }
        blank_streak = 0;

        // ATX heading: 1..=6 '#' followed by a space.
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            let level = hashes as u8;
            let content = trimmed[hashes..].trim_start();
            let (text, mut spans) = inline(content);
            spans.push((0..text.len(), emph_span(true)));
            out.push(MarkdownLine {
                text: text.into(),
                indent: (level as usize - 1) * 2,
                kind: LineKind::Heading(level),
                highlights: spans,
            });
            continue;
        }

        if is_rule(trimmed) {
            out.push(plain("", LineKind::Rule, 0));
            continue;
        }

        // Blockquote: strip the marker, style the whole line italic.
        if let Some(rest) = trimmed.strip_prefix('>') {
            let content = rest.strip_prefix(' ').unwrap_or(rest);
            let (text, mut spans) = inline(content);
            spans.push((0..text.len(), emph_span(false)));
            out.push(MarkdownLine {
                text: text.into(),
                indent: 2,
                kind: LineKind::Quote,
                highlights: spans,
            });
            continue;
        }

        // List item: colored marker, inline-styled content.
        if let Some((marker_len, marker)) = list_marker(trimmed) {
            let content = trimmed[marker_len..].trim_start();
            let pad = trimmed.len() - content.len();
            let (text, spans) = inline(content);
            let mut highlights = Vec::with_capacity(spans.len() + 1);
            highlights.push((
                0..marker.len(),
                HighlightStyle {
                    color: Some(rgb(ACCENT_BLUE).into()),
                    font_weight: Some(gpui::FontWeight::BOLD),
                    ..Default::default()
                },
            ));
            let mut shifted = spans;
            for (range, _) in shifted.iter_mut() {
                range.start += marker.len();
                range.end += marker.len();
            }
            highlights.extend(shifted);
            out.push(MarkdownLine {
                text: format!("{marker}{text}").into(),
                indent: pad,
                kind: LineKind::Body,
                highlights,
            });
            continue;
        }

        let (text, spans) = inline(trimmed);
        out.push(MarkdownLine {
            text: text.into(),
            indent: 0,
            kind: LineKind::Body,
            highlights: spans,
        });
    }
    out
}

/// Inline styling over one line: `**bold**`, `*italic*`, `_italic_`,
/// `` `code` ``, `[label](url)` and `~~strike~~`. Returns the plain text and
/// byte-range spans over it.
fn inline(s: &str) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let mut text = String::with_capacity(s.len());
    let mut spans = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    // Bytes of `s` already copied into `text`; ranges are pushed between
    // `consumed` and the current position so rejected candidates never lose
    // or duplicate the surrounding literal text.
    let mut consumed = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Inline code: content taken literally between single backticks.
        if b == b'`'
            && let Some(rel) = s[i + 1..].find('`')
            && !s[i + 1..i + 1 + rel].is_empty()
        {
            let code = &s[i + 1..i + 1 + rel];
            text.push_str(&s[consumed..i]);
            let start = text.len();
            text.push_str(code);
            spans.push((start..text.len(), code_span()));
            i += rel + 2;
            consumed = i;
            continue;
        }
        // Link: [label](url) — the label is kept, styled as a link; the URL is
        // dropped. A ')' inside the URL makes it malformed: render literally.
        if b == b'['
            && let Some(close) = s[i + 1..].find(']').map(|rel| i + 1 + rel)
            && s[close + 1..].starts_with('(')
        {
            // The URL closes at the parenthesis balancing the opener, so
            // `/a_(b)` survives inside a URL; an unbalanced one never closes
            // and the whole link renders literally.
            let open = close + 1;
            let mut depth = 1usize;
            let mut paren = None;
            for (offset, c) in s[open + 1..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            paren = Some(open + 1 + offset);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(paren) = paren
                && !s[i + 1..close].is_empty()
                && !s[open + 1..paren].is_empty()
            {
                let label = &s[i + 1..close];
                text.push_str(&s[consumed..i]);
                let start = text.len();
                text.push_str(label);
                spans.push((start..text.len(), link_span()));
                i = paren + 1;
                consumed = i;
                continue;
            }
            // Rejected link: advance past the `[` or the loop spins forever.
            i += 1;
            continue;
        }
        // Strikethrough: ~~text~~.
        if s[i..].starts_with("~~")
            && let Some(rel) = s[i + 2..].find("~~")
            && !s[i + 2..i + 2 + rel].is_empty()
        {
            let inner = &s[i + 2..i + 2 + rel];
            text.push_str(&s[consumed..i]);
            let start = text.len();
            text.push_str(inner);
            spans.push((start..text.len(), del_span()));
            i += rel + 4;
            consumed = i;
            continue;
        }
        // Emphasis: **bold**, *italic*, _italic_.
        if b == b'*' || b == b'_' {
            let (marker_len, bold) = if s[i..].starts_with("**") {
                (2, true)
            } else {
                (1, false)
            };
            let closing = if bold { "**" } else { "*" };
            if let Some(rel) = s[i + marker_len..].find(closing) {
                let close = i + marker_len + rel;
                if close > i + marker_len {
                    text.push_str(&s[consumed..i]);
                    let start = text.len();
                    text.push_str(&s[i + marker_len..close]);
                    spans.push((start..text.len(), emph_span(bold)));
                    i = close + marker_len;
                    consumed = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    text.push_str(&s[consumed..]);
    (text, spans)
}

/// A parsed Markdown document: the rendered lines, the syntax-colored source
/// lines (built once, on the worker) and which view is active. The toggle is
/// a plain bool, so switching costs nothing.
pub(crate) struct Document {
    pub(crate) rendered: Vec<MarkdownLine>,
    pub(crate) source: crate::state::code_preview::CodePreview,
    pub(crate) show_rendered: bool,
}

impl Document {
    pub(crate) fn new(path: &std::path::Path, source: &str) -> Self {
        Self {
            rendered: parse(source),
            source: crate::state::code_preview::CodePreview::new(path, source)
                .expect("a .md path always resolves a syntect syntax"),
            show_rendered: true,
        }
    }

    pub(crate) fn toggle(&mut self) {
        self.show_rendered = !self.show_rendered;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(lines: &[MarkdownLine]) -> Vec<String> {
        lines.iter().map(|l| l.text.to_string()).collect()
    }

    #[test]
    fn headings_get_levels_and_bold() {
        let lines = parse("# Title\n\n## Section\nBody");
        assert_eq!(text_of(&lines), ["Title", "", "Section", "Body"]);
        assert_eq!(lines[0].kind, LineKind::Heading(1));
        assert_eq!(lines[0].indent, 0);
        assert_eq!(lines[2].kind, LineKind::Heading(2));
        assert_eq!(lines[2].indent, 2);
        assert!(
            lines[0]
                .highlights
                .iter()
                .any(|(_, style)| style.font_weight == Some(gpui::FontWeight::BOLD))
        );
    }

    #[test]
    fn a_hash_without_space_is_a_paragraph() {
        let lines = parse("#tag");
        assert_eq!(lines[0].kind, LineKind::Body);
        assert_eq!(lines[0].text, "#tag");
    }

    #[test]
    fn inline_spans_cover_the_right_bytes() {
        let (text, spans) = inline("a **bold** b");
        assert_eq!(text, "a bold b");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].0, 2..6);
        assert_eq!(&text[2..6], "bold");
    }

    #[test]
    fn italic_and_code_and_strike() {
        let (text, spans) = inline("*it* and `co` and ~~no~~");
        assert_eq!(text, "it and co and no");
        assert_eq!(spans.len(), 3);
        assert_eq!(
            spans[0].1.font_style,
            Some(gpui::FontStyle::Italic),
            "italic first"
        );
        assert!(spans[1].1.background_color.is_some(), "code tinted");
        assert!(spans[2].1.strikethrough.is_some(), "strike last");
    }

    #[test]
    fn links_keep_the_label_and_drop_the_url() {
        let (text, spans) = inline("see [docs](https://example.net/a) now");
        assert_eq!(text, "see docs now");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].0, 4..8);
        assert!(spans[0].1.underline.is_some());
    }

    #[test]
    fn a_url_with_balanced_parentheses_is_a_link() {
        // CommonMark matches balanced parentheses inside a URL.
        let (text, spans) = inline("[x](https://e.net/a_(b))");
        assert_eq!(text, "x");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].0, 0..1);
    }

    #[test]
    fn an_unbalanced_parenthesis_stays_literal() {
        let (text, spans) = inline("[x](https://e.net/a_(b)");
        assert_eq!(text, "[x](https://e.net/a_(b)");
        assert!(spans.is_empty());
    }

    #[test]
    fn an_empty_label_is_not_a_link() {
        let (text, spans) = inline("[](https://e.net)");
        assert_eq!(text, "[](https://e.net)");
        assert!(spans.is_empty());
    }

    #[test]
    fn unclosed_emphasis_stays_literal() {
        let (text, spans) = inline("a *bold b");
        assert_eq!(text, "a *bold b");
        assert!(spans.is_empty());
    }

    #[test]
    fn fences_render_verbatim_lines() {
        let lines = parse("before\n```rust\nfn main() {}\n```\nafter");
        assert_eq!(text_of(&lines), ["before", "fn main() {}", "after"]);
        assert_eq!(lines[1].kind, LineKind::CodeBlock);
        assert_eq!(lines[2].kind, LineKind::Body);
    }

    #[test]
    fn a_mermaid_block_gets_a_type_caption() {
        let lines = parse("text\n```mermaid\nsequenceDiagram\n A->>B: hi\n```\nafter");
        assert_eq!(
            text_of(&lines),
            [
                "text",
                "mermaid · sequence",
                "sequenceDiagram",
                " A->>B: hi",
                "after"
            ]
        );
        assert_eq!(lines[1].kind, LineKind::Mermaid);
        assert_eq!(lines[2].kind, LineKind::CodeBlock);
    }

    #[test]
    fn every_mermaid_kind_is_captioned_in_a_document() {
        for (keyword, id) in [
            ("graph TD", "flowchart-v2"),
            ("flowchart-elk LR", "flowchart-elk"),
            ("sequenceDiagram", "sequence"),
            ("classDiagram", "classDiagram"),
            ("stateDiagram-v2", "stateDiagram"),
            ("erDiagram", "er"),
            ("gantt", "gantt"),
            ("pie", "pie"),
            ("journey", "journey"),
            ("gitGraph", "gitGraph"),
            ("quadrantChart", "quadrantChart"),
            ("requirementDiagram", "requirement"),
            ("mindmap", "mindmap"),
            ("timeline", "timeline"),
            ("sankey-beta", "sankey"),
            ("xychart-beta", "xychart"),
            ("block-beta", "block"),
            ("packet-beta", "packet"),
            ("kanban", "kanban"),
            ("architecture-beta", "architecture"),
            ("radar-beta", "radar"),
            ("treemap", "treemap"),
            ("venn-beta", "venn"),
            ("eventmodeling", "eventmodeling"),
            ("treeView-beta", "treeView"),
            ("ishikawa", "ishikawa"),
            ("wardley-beta", "wardley"),
            ("cynefin-beta", "cynefin"),
            ("railroad-beta", "railroad"),
            ("railroad-ebnf-beta", "railroad-ebnf"),
            ("railroad-abnf-beta", "railroad-abnf"),
            ("railroad-peg-beta", "railroad-peg"),
            ("usecase-beta", "usecase"),
            ("agentflow-beta", "agentflow"),
            ("swimlane-beta", "swimlane"),
            ("C4Context", "c4"),
            ("info", "info"),
            ("error", "error"),
        ] {
            let document = format!("```mermaid\n{keyword}\nbody\n```");
            let lines = parse(&document);
            assert_eq!(
                lines[0].text,
                format!("mermaid · {id}"),
                "keyword `{keyword}`"
            );
            assert_eq!(lines[0].kind, LineKind::Mermaid, "keyword `{keyword}`");
        }
    }

    #[test]
    fn a_non_mermaid_block_has_no_caption() {
        let lines = parse("```rust\nfn main() {}\n```");
        assert_eq!(text_of(&lines), ["fn main() {}"]);
        let lines = parse("```\nplain\n```");
        assert_eq!(text_of(&lines), ["plain"]);
    }

    #[test]
    fn plantuml_blocks_are_captioned_like_mermaid_ones() {
        let lines = parse("```plantuml\n@startuml\nparticipant A\nA -> B: hi\n@enduml\n```");
        assert_eq!(lines[0].text, "plantuml · sequence");
        assert_eq!(lines[0].kind, LineKind::Mermaid);
        assert_eq!(lines[1].text, "@startuml");
        assert_eq!(lines[1].kind, LineKind::CodeBlock);
    }

    #[test]
    fn plantuml_aliases_and_dedicated_start_tags() {
        let lines = parse("```uml\n@startmindmap\n* root\n@endmindmap\n```");
        assert_eq!(lines[0].text, "plantuml · mindmap");
        let lines = parse("```puml\n@startgantt\n[t] lasts 1d\n@endgantt\n```");
        assert_eq!(lines[0].text, "plantuml · gantt");
    }

    #[test]
    fn plantuml_keyword_inside_startuml_and_delayed_keyword() {
        let lines = parse("```plantuml\n@startuml\nclass Foo\n```\n");
        assert_eq!(lines[0].text, "plantuml · class");
        // A bare @startuml delays: the caption is inserted retroactively.
        let lines = parse("```plantuml\n@startuml\nskinparam x y\nstate s1\n```");
        assert_eq!(lines[0].kind, LineKind::Mermaid);
        assert_eq!(lines[0].text, "plantuml · state");
        assert_eq!(lines[1].kind, LineKind::CodeBlock);
    }

    #[test]
    fn graphviz_blocks_are_captioned_by_shape() {
        let lines = parse("```dot\ndigraph G {\n a -> b;\n}\n```");
        assert_eq!(lines[0].text, "graphviz · directed-graph");
        assert_eq!(lines[0].kind, LineKind::Mermaid);
        let lines = parse("```graphviz\ngraph G { a -- b; }\n```");
        assert_eq!(lines[0].text, "graphviz · undirected-graph");
        let lines = parse("```gv\ndigraph G { node [shape=record]; a [label=\"{1|2}\"]; }\n```");
        assert_eq!(lines[0].text, "graphviz · record-data-structure");
    }

    #[test]
    fn diagram_languages_with_unknown_body_stay_uncaptioned() {
        let lines = parse("```plantuml\njust words\n```");
        assert_eq!(text_of(&lines), ["just words"]);
        let lines = parse("```dot\nno graph here\n```");
        assert_eq!(text_of(&lines), ["no graph here"]);
    }

    #[test]
    fn mermaid_captions_survive_tilde_fences_and_the_source_view_is_untouched() {
        let lines = parse("~~~mermaid\npie title P\n~~~");
        assert_eq!(text_of(&lines), ["mermaid · pie", "pie title P"]);
        // The source view still shows the original text with the fence.
        let document = Document::new(
            std::path::Path::new("doc.md"),
            "```mermaid\ngraph TD\n a-->b\n```",
        );
        assert!(document.source.text.contains("```mermaid"));
    }

    #[test]
    fn an_unknown_language_that_mermaid_would_reject_has_no_caption() {
        let lines = parse("```mermaidish\ngraph TD\n```");
        // `mermaidish` is not the mermaid info string; no caption, code block.
        assert_eq!(text_of(&lines), ["graph TD"]);
    }

    #[test]
    fn tildes_open_and_close_fences_too() {
        let lines = parse("~~~\ncode\n~~~\ntext");
        assert_eq!(text_of(&lines), ["code", "text"]);
        assert_eq!(lines[0].kind, LineKind::CodeBlock);
    }

    #[test]
    fn lists_get_a_colored_marker_and_kept_number() {
        let lines = parse("- first\n12. twelfth");
        assert_eq!(text_of(&lines), ["- first", "12. twelfth"]);
        let marker = &lines[0].highlights[0];
        assert_eq!(marker.0, 0..2, "marker span covers '- '");
        assert!(marker.1.color.is_some());
        assert_eq!(lines[1].highlights[0].0, 0..4, "'12. ' is the marker");
    }

    #[test]
    fn a_dash_without_space_is_not_a_list() {
        let lines = parse("-dash");
        assert_eq!(lines[0].text, "-dash");
        assert_eq!(lines[0].kind, LineKind::Body);
    }

    #[test]
    fn quotes_are_indented_italic_lines() {
        let lines = parse("> said someone");
        assert_eq!(lines[0].kind, LineKind::Quote);
        assert_eq!(lines[0].indent, 2);
        assert_eq!(lines[0].text, "said someone");
        assert!(
            lines[0]
                .highlights
                .iter()
                .any(|(_, style)| style.font_style == Some(gpui::FontStyle::Italic))
        );
    }

    #[test]
    fn rules_render_as_a_rule_line() {
        let lines = parse("above\n---\nbelow");
        assert_eq!(text_of(&lines), ["above", "", "below"]);
        assert_eq!(lines[1].kind, LineKind::Rule);
    }

    #[test]
    fn consecutive_blanks_collapse_to_one_spacer() {
        let lines = parse("a\n\n\n\nb");
        assert_eq!(text_of(&lines), ["a", "", "b"]);
    }

    #[test]
    fn windows_line_endings_are_normalized() {
        let lines = parse("# T\r\ntext\r\n");
        assert_eq!(text_of(&lines), ["T", "text", ""]);
    }

    #[test]
    fn list_content_is_inline_styled_after_the_marker() {
        let lines = parse("- **bold** item");
        assert_eq!(lines[0].text, "- bold item");
        // One span for the marker, one for the bold word, shifted by 2.
        let bold = &lines[0].highlights[1];
        assert_eq!(bold.0, 2..6);
        assert_eq!(bold.1.font_weight, Some(gpui::FontWeight::BOLD));
    }
}
