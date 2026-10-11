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
            } else {
                out.push(plain(line.to_owned(), LineKind::CodeBlock, 0));
            }
            continue;
        }

        if trimmed.len() >= 3 && trimmed.starts_with("```") {
            fence = Some('`');
            continue;
        }
        if trimmed.len() >= 3 && trimmed.starts_with("~~~") {
            fence = Some('~');
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
