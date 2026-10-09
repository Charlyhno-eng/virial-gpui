//! Prepare syntax colors and display whitespace on the preview worker.
use crate::ui::theme::CODE_TEXT;
use gpui::{HighlightStyle, SharedString, rgb};
use std::{ops::Range, path::Path, sync::LazyLock};
use two_face::re_exports::syntect::{
    easy::HighlightLines,
    highlighting::{Color, StyleModifier, Theme, ThemeItem, ThemeSettings},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

// Neon cyan, magenta and violet tokens on the application's dark background.
static THEME: LazyLock<Theme> = LazyLock::new(|| Theme {
    settings: ThemeSettings {
        foreground: Some(syntax_color(CODE_TEXT)),
        ..Default::default()
    },
    scopes: [
        ("comment", 0x8f86b8),
        ("string", 0x5cf3ff),
        ("constant.numeric", 0xc792ff),
        ("constant.language, keyword, storage", 0xff5cce),
        ("keyword.control", 0xc792ff),
        ("entity.name.function, support.function", 0x70ffd8),
        (
            "entity.name.type, entity.name.class, support.type, support.class",
            0xf694ff,
        ),
        ("variable, entity.other.attribute-name", 0xc8e7ff),
        ("entity.name.tag", 0xff5cce),
    ]
    .into_iter()
    .map(|(scope, color)| ThemeItem {
        scope: scope.parse().expect("valid code preview scope"),
        style: StyleModifier {
            foreground: Some(syntax_color(color)),
            ..Default::default()
        },
    })
    .collect(),
    ..Default::default()
});

fn syntax_color(value: u32) -> Color {
    Color {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
        a: 255,
    }
}

pub(crate) struct CodePreview {
    pub(crate) text: SharedString,
    pub(crate) line_numbers: SharedString,
    pub(crate) highlights: Vec<(Range<usize>, HighlightStyle)>,
}

impl CodePreview {
    pub(crate) fn new(path: &Path, source: &str) -> Option<Self> {
        let syntax = path
            .file_name()
            .and_then(|name| name.to_str())
            // Grammars keyed by a different file name than the actual one.
            .and_then(|name| {
                let aliased = match name {
                    "Containerfile" => "Dockerfile",
                    _ => name,
                };
                SYNTAXES.find_syntax_by_extension(aliased)
            })
            .or_else(|| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .and_then(|extension| {
                        SYNTAXES.find_syntax_by_extension(&extension.to_ascii_lowercase())
                    })
            })
            .or_else(|| SYNTAXES.find_syntax_by_first_line(source.lines().next().unwrap_or("")))?;
        if syntax.name == "Plain Text" {
            return None;
        }

        // GPUI renders spaces literally; expand tabs to four-column stops and
        // normalize Windows line endings before computing UTF-8 byte ranges.
        let mut text = String::with_capacity(source.len());
        let mut column = 0;
        for ch in source.replace("\r\n", "\n").chars() {
            match ch {
                '\t' => {
                    let spaces = 4 - column % 4;
                    text.extend(std::iter::repeat_n(' ', spaces));
                    column += spaces;
                }
                '\n' => {
                    text.push(ch);
                    column = 0;
                }
                _ => {
                    text.push(ch);
                    column += 1;
                }
            }
        }

        let mut highlighter = HighlightLines::new(syntax, &THEME);
        let mut highlights = Vec::new();
        let mut offset = 0;
        for line in LinesWithEndings::from(&text) {
            // Keep the text readable even if a grammar cannot parse a line.
            if let Ok(tokens) = highlighter.highlight_line(line, &SYNTAXES) {
                let mut start = offset;
                for (style, token) in tokens {
                    let end = start + token.len();
                    if start < end {
                        let foreground = style.foreground;
                        highlights.push((
                            start..end,
                            HighlightStyle {
                                color: Some(
                                    rgb((u32::from(foreground.r) << 16)
                                        | (u32::from(foreground.g) << 8)
                                        | u32::from(foreground.b))
                                    .into(),
                                ),
                                ..Default::default()
                            },
                        ));
                    }
                    start = end;
                }
            }
            offset += line.len();
        }
        let line_numbers = (1..=text.split('\n').count())
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .into();
        Some(Self {
            text: text.into(),
            line_numbers,
            highlights,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/state/code_preview.rs"]
mod tests;
