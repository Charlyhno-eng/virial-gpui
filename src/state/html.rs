//! Lightweight HTML renderer that turns a page into styled GPUI lines.
//!
//! Same shape as `markdown.rs`: no dependency, no browser engine. The markup
//! is scanned tag by tag, and the document becomes a flat list of display
//! lines — block elements start a new line, inline elements become byte-range
//! `HighlightStyle` spans over the text they wrap. Rendering is deliberately
//! approximate (a file manager preview, not a browser): scripts, styles,
//! comments and attributes are not displayed, unknown or malformed markup
//! degrades to plain text, and the result is always readable.
//!
//! Only inert content is shown: no markup is ever interpreted, no remote
//! resource is fetched, and `<script>`/`<style>` bodies are shown muted at
//! most — they are never executed or laid out as page content.

use gpui::{FontStyle, FontWeight, HighlightStyle, SharedString, rgb, rgba};
use std::collections::HashMap;
use std::ops::Range;

use crate::ui::theme::{ACCENT_BLUE, MUTED};

/// Inline code: the syntect theme's string color on a faint surface tint.
fn code_span() -> HighlightStyle {
    HighlightStyle {
        color: Some(rgb(0x5cf3ff).into()),
        background_color: Some(rgba(0x35314355).into()),
        ..Default::default()
    }
}

/// Links and other navigational attributes.
fn link_span() -> HighlightStyle {
    HighlightStyle {
        color: Some(rgb(ACCENT_BLUE).into()),
        underline: Some(gpui::UnderlineStyle::default()),
        ..Default::default()
    }
}

/// A list bullet or number: the accent color, like the Markdown markers.
fn list_marker_span() -> HighlightStyle {
    HighlightStyle {
        color: Some(rgb(ACCENT_BLUE).into()),
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    }
}

/// Text the page does not show (a script body, a hidden element): muted rather
/// than silently dropped, so the reader sees the file had it.
fn hidden_span() -> HighlightStyle {
    HighlightStyle {
        color: Some(rgb(MUTED).into()),
        font_style: Some(FontStyle::Italic),
        ..Default::default()
    }
}

/// Block-level appearance of a rendered line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LineKind {
    Body,
    /// `h1`..`h6` heading, level 1..=6.
    Heading(u8),
    /// The document `<title>`, shown as a caption above the page.
    Title,
    /// A `blockquote` line.
    Quote,
    /// A `pre` block.
    CodeBlock,
    /// A list item; its nesting is carried by `indent`.
    ListItem,
    /// A horizontal rule.
    Rule,
}

/// One rendered line: text, left indent, block style and inline spans.
pub(crate) struct HtmlLine {
    pub(crate) text: SharedString,
    /// Spaces of left padding, applied by the renderer.
    pub(crate) indent: usize,
    pub(crate) kind: LineKind,
    pub(crate) highlights: Vec<(Range<usize>, HighlightStyle)>,
}

impl HtmlLine {
    fn plain(text: impl Into<SharedString>, kind: LineKind, indent: usize) -> Self {
        Self {
            text: text.into(),
            indent,
            kind,
            highlights: Vec::new(),
        }
    }

    /// Is this the blank spacer between two blocks?
    fn is_spacer(&self) -> bool {
        self.text.is_empty() && self.kind == LineKind::Body && self.indent == 0
    }
}

/// A parsed HTML page: the rendered lines and the syntax-colored source lines,
/// both built once on the worker. Which view is active lives in the `Preview`
/// variant, so the toggle and the renderer can never disagree.
pub(crate) struct Document {
    pub(crate) rendered: Vec<HtmlLine>,
    pub(crate) source: crate::state::code_preview::CodePreview,
}

impl Document {
    pub(crate) fn new(path: &std::path::Path, source: &str) -> Self {
        Self {
            rendered: parse(source),
            // A missing grammar must never panic the worker: the source view
            // then falls back to plain text instead of taking the preview down.
            source: crate::state::code_preview::CodePreview::new(path, source)
                .unwrap_or_else(|| crate::state::code_preview::CodePreview::plain(source)),
        }
    }
}

/// Elements whose text content is code or styling, never prose.
const SCRIPTED: [&str; 7] = ["script", "style", "template", "svg", "noscript", "iframe", "object"];

/// Elements rendered as their own block: each one ends the pending line.
const BLOCKS: [&str; 47] = [
    "html", "head", "body", "div", "section", "article", "header", "footer", "main", "nav", "aside",
    "h1", "h2", "h3", "h4", "h5", "h6", "p", "blockquote", "pre", "ul", "ol", "li", "dl", "dt",
    "dd", "table", "thead", "tbody", "tfoot", "tr", "td", "th", "caption", "colgroup", "form",
    "fieldset", "legend", "label", "figure", "figcaption", "address", "details", "summary",
    "dialog", "hgroup",
];

/// Elements that never have content and never close: handled in place.
const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Inline elements: they style the text they wrap instead of starting a line.
fn inline_style(name: &str) -> Option<HighlightStyle> {
    match name {
        "b" | "strong" => Some(HighlightStyle {
            font_weight: Some(FontWeight::BOLD),
            ..Default::default()
        }),
        "i" | "em" | "cite" | "dfn" | "var" => Some(HighlightStyle {
            font_style: Some(FontStyle::Italic),
            ..Default::default()
        }),
        "code" | "kbd" | "samp" | "tt" => Some(code_span()),
        "a" | "link" => Some(link_span()),
        "del" | "s" | "strike" => Some(HighlightStyle {
            strikethrough: Some(gpui::StrikethroughStyle::default()),
            ..Default::default()
        }),
        "mark" => Some(HighlightStyle {
            background_color: Some(rgba(0xc792ff33).into()),
            ..Default::default()
        }),
        _ => None,
    }
}

/// Read a start tag's attributes into a map keyed by their lowercased names.
fn attributes(source: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Names start after whitespace.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        if i == start {
            break;
        }
        let name = source[start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            map.insert(name, String::new());
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let value = if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
            let quote = bytes[i];
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let value = source[start..i.min(bytes.len())].to_owned();
            i += 1;
            value
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            source[start..i].to_owned()
        };
        map.insert(name, value);
    }
    map
}

/// Decode the entities that matter when reading text: the XML predefined
/// names, the numeric references, and `&nbsp;`. Anything unknown stays typed,
/// so a lone `&` in a query string survives untouched.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find('&') {
        out.push_str(&rest[..position]);
        rest = &rest[position + 1..];
        // An entity reference closes at the first ';' of the next 10 bytes.
        let end = rest[..rest.len().min(10)].find(';');
        let decoded = end.and_then(|end| {
            let name = &rest[..end];
            match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ if name.starts_with("#x") || name.starts_with("#X") => {
                    u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32)
                }
                _ if name.starts_with('#') => name[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            }
        });
        match (end, decoded) {
            (Some(end), Some(character)) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            _ => out.push('&'),
        }
    }
    out.push_str(rest);
    out
}

/// Find the `>` that closes a tag, ignoring the ones inside quoted values, so
/// `<p title="a > b">` stays one tag.
fn tag_end(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut quote = None;
    for (index, &byte) in bytes.iter().enumerate() {
        match (quote, byte) {
            (None, b'"') | (None, b'\'') => quote = Some(byte),
            (Some(open), byte) if byte == open => quote = None,
            (None, b'>') => return Some(index),
            _ => {}
        }
    }
    None
}

/// Parse a whole document into display lines.
pub(crate) fn parse(source: &str) -> Vec<HtmlLine> {
    let source = source.replace("\r\n", "\n");
    let mut out: Vec<HtmlLine> = Vec::new();
    // The line being filled by the current inline content.
    let mut line = String::new();
    let mut spans: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    let mut indent = 0usize;
    let mut kind = LineKind::Body;
    // Open elements, innermost last; the second field is the inline style.
    let mut open: Vec<(String, Option<HighlightStyle>)> = Vec::new();
    let mut list_depth = 0usize;
    let mut pre_depth = 0usize;
    let mut scripted_depth = 0usize;
    let mut title: Option<HtmlLine> = None;
    let mut index = 0usize;

    /// End the pending line and make sure one blank spacer follows it.
    fn close_line(
        out: &mut Vec<HtmlLine>,
        line: &mut String,
        spans: &mut Vec<(Range<usize>, HighlightStyle)>,
        indent: &mut usize,
        kind: LineKind,
    ) {
        if !line.is_empty() {
            out.push(HtmlLine {
                text: std::mem::take(line).into(),
                indent: *indent,
                kind,
                highlights: std::mem::take(spans),
            });
        }
        *indent = 0;
        spacer(out);
    }

    fn spacer(out: &mut Vec<HtmlLine>) {
        if !out.is_empty() && !out.last().is_some_and(HtmlLine::is_spacer) {
            out.push(HtmlLine::plain("", LineKind::Body, 0));
        }
    }

    /// Append text to the pending line, styled by the innermost inline tag and
    /// muted inside a scripted or hidden element.
    fn push_text(
        line: &mut String,
        spans: &mut Vec<(Range<usize>, HighlightStyle)>,
        text: &str,
        style: Option<HighlightStyle>,
        hidden: bool,
        verbatim: bool,
    ) {
        if text.is_empty() {
            return;
        }
        let text = decode_entities(text);
        if !verbatim && text.trim().is_empty() {
            // Layout whitespace between tags: collapse it to one space so a
            // paragraph wrapped across lines stays on one display line.
            if !line.is_empty() && !line.ends_with(' ') {
                line.push(' ');
            }
            return;
        }
        let start = line.len();
        line.push_str(&text);
        let end = line.len();
        if hidden {
            spans.push((start..end, hidden_span()));
        } else if let Some(style) = style {
            spans.push((start..end, style));
        }
    }

    while index < source.len() {
        let rest = &source[index..];
        let style = open.iter().rev().find_map(|(_, style)| *style);
        let Some(position) = rest.find('<') else {
            push_text(
                &mut line,
                &mut spans,
                rest,
                style,
                scripted_depth > 0,
                pre_depth > 0,
            );
            break;
        };
        if position > 0 {
            push_text(
                &mut line,
                &mut spans,
                &rest[..position],
                style,
                scripted_depth > 0,
                pre_depth > 0,
            );
            index += position;
        }
        let rest = &source[index..];
        // Comments: content the page never shows, dropped entirely.
        if rest.starts_with("<!--") {
            index += rest[4..].find("-->").map_or(0, |at| 4 + at + 3);
            continue;
        }
        // Doctype and processing instructions.
        if rest.starts_with("<!") || rest.starts_with("<?") {
            index += tag_end(rest).map_or(rest.len(), |end| end + 1);
            continue;
        }
        if rest[1..].starts_with('/') {
            // A closing tag.
            let Some(end) = tag_end(rest) else {
                line.push('<');
                index += 1;
                continue;
            };
            let name = rest[2..end].trim().to_ascii_lowercase();
            index += end + 1;
            // Close up to the matching open tag: an unclosed inner tag is
            // simply dropped, which keeps the output flat and predictable.
            if let Some(depth) = open.iter().rposition(|(tag, _)| *tag == name) {
                let closed: Vec<_> = open.split_off(depth);
                let scripted = closed
                    .iter()
                    .filter(|(tag, _)| SCRIPTED.contains(&tag.as_str()))
                    .count();
                let pre = closed.iter().filter(|(tag, _)| tag == "pre").count();
                scripted_depth = scripted_depth.saturating_sub(scripted);
                pre_depth = pre_depth.saturating_sub(pre);
            }
            if name == "title" {
                if !line.trim().is_empty() {
                    title = Some(HtmlLine {
                        text: line.trim().into(),
                        indent: 0,
                        kind: LineKind::Title,
                        highlights: Vec::new(),
                    });
                }
                line.clear();
                spans.clear();
            } else if SCRIPTED.contains(&name.as_str())
                || VOID.contains(&name.as_str())
                || inline_style(&name).is_some()
            {
                // Inline and inert: the style ends with its own tag.
            } else {
                close_line(&mut out, &mut line, &mut spans, &mut indent, LineKind::Body);
            }
            kind = LineKind::Body;
            continue;
        }
        // A start tag: read its name, stopping at the first non-name byte.
        let name: String = rest[1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == ':')
            .collect::<String>()
            .to_ascii_lowercase();
        if name.is_empty() {
            // A bare `<` in prose (a comparison, untyped markup).
            line.push('<');
            index += 1;
            continue;
        }
        let Some(end) = tag_end(rest) else {
            // An unterminated tag: the rest of the file is text.
            line.push('<');
            index += 1;
            continue;
        };
        let inside = &rest[name.len() + 1..end];
        let self_closing = inside.trim_end().ends_with('/');
        let attributes = attributes(inside);
        index += end + 1;

        if VOID.contains(&name.as_str()) {
            match name.as_str() {
                "br" => close_line(&mut out, &mut line, &mut spans, &mut indent, LineKind::Body),
                "hr" => {
                    close_line(&mut out, &mut line, &mut spans, &mut indent, LineKind::Body);
                    out.push(HtmlLine::plain("", LineKind::Rule, 0));
                    spacer(&mut out);
                }
                "img" => {
                    // An image contributes its alternative text, or its source
                    // when it has none. The picture itself is never loaded.
                    let alt = attributes.get("alt").map(String::as_str).unwrap_or("");
                    let alt = alt.trim();
                    let src = attributes.get("src").map(String::as_str).unwrap_or("");
                    let label = if alt.is_empty() { src.trim() } else { alt };
                    if !label.is_empty() {
                        let start = line.len();
                        line.push('[');
                        line.push_str(label);
                        line.push(']');
                        spans.push((start..line.len(), hidden_span()));
                    }
                }
                _ => {}
            }
            continue;
        }

        if SCRIPTED.contains(&name.as_str()) {
            scripted_depth += 1;
            open.push((name, None));
            continue;
        }
        let block = BLOCKS.contains(&name.as_str());
        if block {
            close_line(&mut out, &mut line, &mut spans, &mut indent, LineKind::Body);
        }
        let hidden = attributes.contains_key("hidden")
            || attributes
                .get("style")
                .is_some_and(|style| style.replace(' ', "").contains("display:none"));
        let style = inline_style(&name);
        match name.as_str() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                kind = LineKind::Heading(name[1..].parse().unwrap_or(6));
            }
            "blockquote" => kind = LineKind::Quote,
            "pre" => {
                pre_depth += 1;
                kind = LineKind::CodeBlock;
            }
            "ul" | "ol" => {
                list_depth += 1;
                list_items = 0;
            }
            "li" => {
                kind = LineKind::ListItem;
                indent = list_depth * 2;
            }
            _ => kind = LineKind::Body,
        }
        if name == "li" && !self_closing {
            // gpui 0.2 has no list marker: prefix the bullet here, colored by
            // the renderer through a span over the same bytes.
            let start = line.len();
            let ordered = open
                .iter()
                .rev()
                .find(|(tag, _)| tag == "ul" || tag == "ol")
                .is_some_and(|(tag, _)| tag == "ol");
            let marker = if ordered {
                format!("{}. ", list_items + 1)
            } else {
                "• ".to_owned()
            };
            list_items += 1;
            line.push_str(&marker);
            spans.push((start..line.len(), list_marker_span()));
        }
        open.push((name, if hidden { None } else { style }));
        if self_closing {
            // `<span/>` and friends: nothing stays open.
            if let Some((tag, _)) = open.pop() {
                pre_depth = pre_depth.saturating_sub(usize::from(tag == "pre"));
            }
            if block {
                close_line(&mut out, &mut line, &mut spans, &mut indent, LineKind::Body);
                kind = LineKind::Body;
            }
        }
    }
    if !line.is_empty() {
        out.push(HtmlLine {
            text: line.into(),
            indent,
            kind,
            highlights: spans,
        });
    }
    while out.last().is_some_and(HtmlLine::is_spacer) {
        out.pop();
    }
    // The document title becomes the caption of the rendered page.
    if let Some(title) = title {
        out.insert(0, title);
        spacer(&mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(lines: &[HtmlLine]) -> Vec<String> {
        lines.iter().map(|line| line.text.to_string()).collect()
    }

    #[test]
    fn headings_and_paragraphs_become_separate_lines() {
        let lines = parse("<h1>Title</h1><p>Hello</p>");
        assert_eq!(text_of(&lines), ["Title", "", "Hello"]);
        assert_eq!(lines[0].kind, LineKind::Heading(1));
        assert_eq!(lines[2].kind, LineKind::Body);
    }

    #[test]
    fn heading_levels_come_from_the_tag_name() {
        let lines = parse("<h3>Deep</h3>");
        assert_eq!(lines[0].kind, LineKind::Heading(3));
    }

    #[test]
    fn a_malformed_tag_name_is_not_a_heading() {
        let lines = parse("<h3x>No</h3x>");
        assert!(lines.is_empty() || lines[0].kind != LineKind::Heading(3));
    }

    #[test]
    fn inline_tags_style_the_text_they_wrap() {
        let lines = parse("<p>a <b>bold</b> word</p>");
        assert_eq!(lines[0].text, "a bold word");
        assert_eq!(lines[0].highlights[0].0, 2..6);
        assert_eq!(
            lines[0].highlights[0].1.font_weight,
            Some(FontWeight::BOLD)
        );
    }

    #[test]
    fn links_keep_their_label_and_underline_it() {
        let lines = parse("<a href=\"https://e.net\">docs</a>");
        assert_eq!(lines[0].text, "docs");
        assert!(lines[0].highlights[0].1.underline.is_some());
    }

    #[test]
    fn an_empty_link_does_not_invent_text() {
        assert!(parse("<a href=\"https://e.net\"></a>").is_empty());
    }

    #[test]
    fn scripts_and_styles_are_not_page_content() {
        let lines = parse("<style>body { color: red }</style><p>Visible</p>");
        assert!(
            !text_of(&lines).iter().any(|line| line.contains("color: red")),
            "CSS is not page content: {lines:?}"
        );
        assert!(text_of(&lines).contains(&"Visible".to_owned()));
        let lines = parse("<script>alert('x')</script><p>After</p>");
        assert!(
            !text_of(&lines).iter().any(|line| line.contains("alert")),
            "a script body is not page content: {lines:?}"
        );
        assert!(text_of(&lines).contains(&"After".to_owned()));
    }

    #[test]
    fn comments_are_dropped() {
        let lines = parse("<p>before<!-- hidden note -->after</p>");
        assert_eq!(lines[0].text, "beforeafter");
    }

    #[test]
    fn list_items_are_bulleted_and_indented() {
        let lines = parse("<ul><li>first</li><li>second</li></ul>");
        let items: Vec<_> = lines
            .iter()
            .filter(|line| line.kind == LineKind::ListItem)
            .collect();
        assert_eq!(items.len(), 2);
        assert!(items[0].text.ends_with("first"), "bullet prefix: {items:?}");
        assert!(items[1].text.ends_with("second"));
        assert!(items[0].indent > 0, "list items are indented");
        assert!(
            items[0].highlights.iter().any(|(range, style)| range.end == 2
                && style.color.is_some()),
            "the bullet itself is colored: {items:?}"
        );
    }

    #[test]
    fn ordered_lists_are_numbered() {
        let lines = parse("<ol><li>a</li><li>b</li></ol>");
        let items: Vec<_> = lines
            .iter()
            .filter(|line| line.kind == LineKind::ListItem)
            .collect();
        assert_eq!(items.len(), 2);
        assert!(items[0].text.starts_with("1. "), "{items:?}");
        assert!(items[1].text.starts_with("2. "), "{items:?}");
    }

    #[test]
    fn a_nested_list_is_indented_further() {
        let lines = parse("<ul><li>a<ul><li>b</li></ul></li></ul>");
        let items: Vec<_> = lines
            .iter()
            .filter(|line| line.kind == LineKind::ListItem)
            .collect();
        assert_eq!(items.len(), 2);
        assert!(
            items[1].indent > items[0].indent,
            "the nested item is indented further: {items:?}"
        );
    }

    #[test]
    fn blockquotes_and_rules_get_their_own_kinds() {
        let lines = parse("<blockquote>quoted</blockquote><hr><p>after</p>");
        let quote = lines
            .iter()
            .find(|line| line.text == "quoted")
            .expect("quote line");
        assert_eq!(quote.kind, LineKind::Quote);
        assert!(lines.iter().any(|line| line.kind == LineKind::Rule));
    }

    #[test]
    fn table_cells_are_kept_as_lines() {
        let lines = parse("<table><tr><td>a</td><td>b</td></tr></table>");
        assert!(
            lines.iter().any(|line| line.text.contains('a')),
            "table cells are content: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.text.contains('b')),
            "table cells are content: {lines:?}"
        );
    }

    #[test]
    fn the_document_title_is_shown_first() {
        let lines = parse("<html><head><title>Home</title></head><body><p>Hi</p></body></html>");
        assert_eq!(lines[0].text, "Home");
        assert_eq!(lines[0].kind, LineKind::Title);
    }

    #[test]
    fn preformatted_text_keeps_its_whitespace() {
        let lines = parse("<pre>  indented\n    more</pre>");
        assert!(lines[0].text.starts_with("  indented"), "{lines:?}");
        assert_eq!(lines[0].kind, LineKind::CodeBlock);
    }

    #[test]
    fn entities_are_decoded() {
        assert_eq!(decode_entities("a &amp; b"), "a & b");
        assert_eq!(decode_entities("&lt;div&gt;"), "<div>");
        assert_eq!(decode_entities("&#65;&#x42;"), "AB");
        assert_eq!(decode_entities("caf&eacute;"), "caf&eacute;");
        assert_eq!(decode_entities("plain"), "plain");
    }

    #[test]
    fn a_bare_ampersand_stays_literal() {
        assert_eq!(decode_entities("a & b"), "a & b");
        assert_eq!(decode_entities("?x=1&y=2"), "?x=1&y=2");
    }

    #[test]
    fn attributes_are_read_with_either_quote_style() {
        let map = attributes(" href='https://e.net/a b' ID=main disabled");
        assert_eq!(
            map.get("href").map(String::as_str),
            Some("https://e.net/a b")
        );
        assert_eq!(map.get("id").map(String::as_str), Some("main"));
        assert!(map.contains_key("disabled"));
    }

    #[test]
    fn a_greater_than_inside_an_attribute_does_not_split_the_tag() {
        let lines = parse("<p title=\"a > b\">text</p>");
        assert!(
            lines.iter().any(|line| line.text.contains("text")),
            "the tag is read whole: {lines:?}"
        );
    }

    #[test]
    fn an_image_contributes_its_alt_text() {
        let lines = parse("<p><img src=\"logo.png\" alt=\"Logo\"></p>");
        assert!(
            lines.iter().any(|line| line.text.contains("Logo")),
            "alt text stands in for the image: {lines:?}"
        );
    }

    #[test]
    fn unclosed_markup_keeps_its_text() {
        let lines = parse("<p>text <b>bold</p>");
        assert!(
            lines.iter().any(|line| line.text.contains("bold")),
            "an unclosed tag keeps its text: {lines:?}"
        );
    }

    #[test]
    fn a_bare_left_angle_bracket_is_text() {
        let lines = parse("<p>5 < 6</p>");
        assert_eq!(lines[0].text, "5 < 6");
    }

    #[test]
    fn tag_names_are_case_insensitive() {
        let lines = parse("<H1>Title</H1><STRONG>bold</STRONG>");
        assert_eq!(lines[0].kind, LineKind::Heading(1));
        assert!(
            lines
                .iter()
                .any(|line| line.highlights.iter().any(|(_, style)| style
                    .font_weight
                    .is_some())),
            "a closing </STRONG> still ends the style: {lines:?}"
        );
    }

    #[test]
    fn hidden_elements_are_muted_not_shown_as_prose() {
        let lines = parse("<div style=\"display:none\">gone</div><p>here</p>");
        let hidden = lines
            .iter()
            .find(|line| line.text.contains("gone"))
            .expect("hidden line");
        assert!(
            hidden
                .highlights
                .iter()
                .any(|(_, style)| style.font_style.is_some()),
            "hidden text is marked as not visible: {hidden:?}"
        );
    }

    #[test]
    fn windows_line_endings_are_normalized() {
        let lines = parse("<p>one</p>\r\n<p>two</p>\r\n");
        assert_eq!(text_of(&lines), ["one", "", "two"]);
    }
}