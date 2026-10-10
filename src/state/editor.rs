//! Editing engine behind the code view: line buffer, cursor, selection and a
//! coalescing undo history. Pure text logic with no GPUI dependency, so it is
//! unit tested on its own.
//!
//! Invariant: the document text is always `lines.join("\n")`. A file ending in
//! a newline simply has an empty last line, so a round trip is exact and no
//! edit needs to special-case the end of the file.

use crate::infrastructure::editor_io::{DocumentFormat, LineEnding};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// One replaced span plus the text before and after it, so undo and redo are
/// the same operation with the other side of the pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub range: Range<usize>,
    pub before: String,
    pub after: String,
}

/// Every change of one user action, applied from the last offset backwards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub changes: Vec<Change>,
}

/// Longest a history stays useful before the oldest steps are dropped.
const MAX_HISTORY: usize = 500;
/// A run of typed characters longer than this becomes several undo steps.
const COALESCE_LIMIT: usize = 64;

#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl History {
    /// Record one action. Consecutive single-character typing merges into the
    /// previous step, so one Ctrl+Z removes the word just typed.
    pub fn record(&mut self, edit: Edit, typing: bool) {
        if edit.changes.is_empty() {
            return;
        }
        self.redo.clear();
        if typing && let Some(last) = self.undo.last_mut()
            && edit.changes.len() == 1
            && last.changes.len() == 1
        {
            let previous = &last.changes[0];
            let current = &edit.changes[0];
            let contiguous = previous.range.end == current.range.start
                && previous.after.len() + current.after.len() <= COALESCE_LIMIT
                && previous.after.chars().count() == 1
                && current.after.chars().count() == 1;
            if contiguous {
                previous.range.end = current.range.end;
                previous.after.push_str(&current.after);
                return;
            }
        }
        self.undo.push(edit);
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// The document: lines without terminators, plus the format to save it with.
pub struct Buffer {
    lines: Vec<String>,
    pub format: DocumentFormat,
    /// Bumped on every change so the view knows a re-highlight is due.
    pub revision: u64,
}

impl Buffer {
    pub fn new(text: &str, format: DocumentFormat) -> Self {
        Self {
            lines: text
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .split('\n')
                .map(str::to_owned)
                .collect(),
            format,
            revision: 0,
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, index: usize) -> &str {
        self.lines.get(index).map_or("", String::as_str)
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The exact text to save; `lines.join("\n")` is the whole document.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Byte offset where a line starts, the unit every range is expressed in.
    pub fn line_start(&self, line: usize) -> usize {
        self.lines
            .iter()
            .take(line)
            .map(|text| text.len() + 1)
            .sum()
    }

    /// Line containing a byte offset. An offset sitting exactly on the newline
    /// belongs to the line it terminates, where a cursor is allowed to rest.
    pub fn line_of(&self, offset: usize) -> usize {
        let mut start = 0;
        for (index, text) in self.lines.iter().enumerate() {
            if offset <= start + text.len() {
                return index;
            }
            start += text.len() + 1;
        }
        self.lines.len() - 1
    }

    /// Clamp a byte offset into the buffer and off any newline.
    pub fn clamp_offset(&self, offset: usize) -> usize {
        let offset = offset.min(self.text().len());
        let line = self.line_of(offset);
        let start = self.line_start(line);
        let inside = (offset - start).min(self.line(line).len());
        // Never land in the middle of a multi-byte character.
        let mut inside = floor_boundary(self.line(line), inside);
        if inside == 0 && line > 0 && offset < start {
            inside = 0;
        }
        start + inside
    }

    /// Character column (1-based for display) of a byte offset.
    pub fn column(&self, offset: usize) -> usize {
        let line = self.line_of(self.clamp_offset(offset));
        let start = self.line_start(line);
        char_count(&self.line(line)[..offset.saturating_sub(start).min(self.line(line).len())]) + 1
    }

    pub fn line_of_offset(&self, offset: usize) -> usize {
        self.line_of(self.clamp_offset(offset))
    }

    /// Apply one replacement, returning the change so it can be undone.
    pub fn replace(&mut self, range: Range<usize>, after: &str) -> Change {
        let text = self.text();
        let start = self.clamp_offset(range.start);
        let end = self.clamp_offset(range.end).max(start);
        let change = Change {
            range: start..end,
            before: text[start..end].to_owned(),
            after: after.to_owned(),
        };
        let head = &text[..start];
        let tail = &text[end..];
        let joined = format!("{head}{}{tail}", change.after.replace("\r\n", "\n"));
        self.lines = joined
            .split('\n')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        self.revision = self.revision.wrapping_add(1);
        change
    }

    pub fn text_in(&self, range: Range<usize>) -> String {
        let text = self.text();
        let start = self.clamp_offset(range.start);
        let end = self.clamp_offset(range.end).max(start);
        text[start..end].to_owned()
    }
}

/// The editable document: buffer, cursor, selection and history.
pub struct Editor {
    buffer: Buffer,
    /// Where the caret is, as a byte offset.
    cursor: usize,
    /// Where the selection started; equal to `cursor` when nothing is selected.
    anchor: usize,
    history: History,
    /// Text as loaded, to answer "is this modified?".
    saved: String,
    pub indent: String,
}

impl Editor {
    pub fn new(text: &str, format: DocumentFormat) -> Self {
        Self {
            buffer: Buffer::new(text, format),
            cursor: 0,
            anchor: 0,
            history: History::default(),
            saved: text.replace("\r\n", "\n").replace('\r', "\n"),
            indent: "    ".to_owned(),
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn text(&self) -> String {
        self.buffer.text()
    }

    pub fn revision(&self) -> u64 {
        self.buffer.revision
    }

    pub fn line_ending(&self) -> LineEnding {
        self.buffer.format.line_ending
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn anchor(&self) -> usize {
        self.anchor
    }

    /// The selected range, always ordered.
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    pub fn has_selection(&self) -> bool {
        self.anchor != self.cursor
    }

    /// Line and column for the status bar, both 1-based.
    pub fn line_column(&self) -> (usize, usize) {
        let cursor = self.buffer.clamp_offset(self.cursor);
        (self.buffer.line_of(cursor) + 1, self.buffer.column(cursor))
    }

    pub fn is_dirty(&self) -> bool {
        self.text() != self.saved
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn set_cursor(&mut self, offset: usize, extend: bool) {
        let offset = self.buffer.clamp_offset(offset);
        if !extend {
            self.anchor = offset;
        }
        self.cursor = offset;
    }

    /// Replace the selection (or insert at the caret) and leave the caret after
    /// the inserted text. `typing` marks single characters, which coalesce.
    pub fn insert(&mut self, text: &str) {
        let selection = self.selection();
        let change = self.buffer.replace(selection, text);
        let end = change.range.start + change.after.len();
        let edit = Edit {
            changes: vec![change],
        };
        self.history.record(edit, typing);
        self.cursor = end;
        self.anchor = end;
    }

    /// Delete the selection, or the unit before/after the caret.
    pub fn delete(&mut self, backwards: bool) {
        let selection = self.selection();
        if !selection.is_empty() {
            self.apply_ranges(vec![selection]);
            return;
        }
        let cursor = self.buffer.clamp_offset(self.cursor);
        if backwards {
            if cursor == 0 {
                return;
            }
            // A whole grapheme goes at once, so an emoji is not cut in half.
            let text = self.text();
            let previous = text[..cursor]
                .grapheme_indices(true)
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(cursor - 1);
            self.apply_ranges(vec![previous..cursor]);
        } else {
            let text = self.text();
            let next = text[cursor..]
                .grapheme_indices(true)
                .nth(1)
                .map(|(index, _)| cursor + index)
                .unwrap_or(text.len());
            if next <= cursor {
                return;
            }
            self.apply_ranges(vec![cursor..next]);
        }
    }

    /// Apply several ranges as one undoable step, back to front.
    fn apply_ranges(&mut self, ranges: Vec<Range<usize>>) {
        let mut ordered = ranges;
        ordered.sort_by(|left, right| right.start.cmp(&left.start));
        let mut cursor = None;
        let mut changes = Vec::with_capacity(ordered.len());
        for range in ordered {
            let change = self.buffer.replace(range, "");
            cursor = Some(change.range.start);
            changes.push(change);
        }
        let cursor = cursor.unwrap_or(self.cursor);
        let edit = Edit { changes };
        self.history.record(edit, false);
        self.cursor = cursor;
        self.anchor = cursor;
    }

    /// Insert a newline, copying the leading whitespace of the current line so
    /// the next line keeps its indentation.
    pub fn newline(&mut self, eol: LineEnding) {
        let cursor = self.buffer.clamp_offset(self.cursor);
        let line = self.buffer.line_of(cursor);
        let start = self.buffer.line_start(line);
        let inside = cursor - start;
        let text = self.buffer.line(line).to_owned();
        let leading: String = text
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .collect();
        let braces = text[..inside].trim_end().ends_with(['{', '(', '[']);
        let indent = if braces {
            format!("{leading}{}", self.indent)
        } else {
            leading
        };
        self.insert(&format!("{eol}{indent}"));
    }

    /// Indent or outdent every line the selection touches.
    pub fn shift(&mut self, outdent: bool) {
        let text = self.text();
        let selection = self.selection();
        let first = self.buffer.line_of(selection.start);
        let last = self.buffer.line_of(selection.end);
        let mut changes = Vec::new();
        for line in first..=last {
            let start = self.buffer.line_start(line);
            let content = text[start..].split('\n').next().unwrap_or("");
            let (range, after) = if outdent {
                let removed = content
                    .chars()
                    .take_while(|ch| *ch == ' ' || *ch == '\t')
                    .map(char::len_utf8)
                    .sum::<usize>();
                let removed = if content.starts_with('\t') {
                    1
                } else {
                    removed.min(self.indent.len())
                };
                (start..start + removed, String::new())
            } else {
                (start..start, self.indent.clone())
            };
            if !outdent || !range.is_empty() {
                let change = self.buffer.replace(range, &after);
                changes.push((change.range.start, change.after.len(), change));
            }
        }
        if changes.is_empty() {
            return;
        }
        changes.sort_by(|left, right| right.0.cmp(&left.0));
        let width: usize = changes.iter().map(|(_, after, _)| after.len()).sum();
        let edit = Edit {
            changes: changes.into_iter().map(|(_, _, change)| change).collect(),
        };
        self.history.record(edit, false);
        let first = self.buffer.clamp_offset(selection.start);
        self.cursor = (first + width).min(self.buffer.text().len());
        self.anchor = self.cursor;
    }

    pub fn undo(&mut self) {
        let Some(edit) = self.history.undo.pop() else {
            return;
        };
        let mut cursor = None;
        for change in edit.changes.iter().rev() {
            let applied = self.buffer.replace(change.range.clone(), &change.before);
            cursor = Some(applied.range.start);
        }
        self.history.redo.push(edit);
        let cursor = cursor.unwrap_or(self.cursor);
        self.cursor = cursor;
        self.anchor = cursor;
    }

    pub fn redo(&mut self) {
        let Some(edit) = self.history.redo.pop() else {
            return;
        };
        let mut cursor = None;
        for change in edit.changes.iter().rev() {
            let applied = self.buffer.replace(change.range.clone(), &change.after);
            cursor = Some(applied.range.end);
        }
        self.history.undo.push(edit);
        let cursor = cursor.unwrap_or(self.cursor);
        self.cursor = cursor;
        self.anchor = cursor;
    }

    /// Forget the saved state so `is_dirty` reports clean after a write.
    pub fn mark_saved(&mut self) {
        self.saved = self.text();
    }

    /// Move the caret by whole characters or lines, keeping the column.
    pub fn move_by(&mut self, direction: Direction, extend: bool, lines: Option<usize>) {
        let text = self.text();
        let cursor = self.buffer.clamp_offset(self.cursor);
        let line = self.buffer.line_of(cursor);
        let start = self.buffer.line_start(line);
        let column = cursor - start;
        let target = match direction {
            Direction::Left => cursor
                .checked_sub(1)
                .filter(|offset| text.is_char_boundary(*offset))
                .unwrap_or(0),
            Direction::Right => text[cursor..]
                .chars()
                .next()
                .map(|ch| cursor + ch.len_utf8())
                .unwrap_or(cursor),
            Direction::WordLeft => word_left(&text, cursor),
            Direction::WordRight => word_right(&text, cursor),
            Direction::LineStart => start,
            Direction::LineEnd => start + self.buffer.line(line).len(),
            Direction::DocumentStart => 0,
            Direction::DocumentEnd => text.len(),
            Direction::LineUp | Direction::LineDown => {
                let step = lines.unwrap_or(1);
                let target_line = match direction {
                    Direction::LineUp => line.checked_sub(step),
                    _ => Some((line + step).min(self.buffer.line_count() - 1)),
                };
                match target_line {
                    // Keep the column when the target line is long enough.
                    Some(target_line) => {
                        let target_start = self.buffer.line_start(target_line);
                        let width = self.buffer.line(target_line).len();
                        let column = char_count(&self.buffer.line(line)[..column.min(self.buffer.line(line).len())]);
                        target_start + byte_index(self.buffer.line(target_line), column).min(width)
                    }
                    None => cursor,
                }
            }
        };
        self.set_cursor(target, extend);
    }

    /// Select the next occurrence of the current selection, as an IDE does.
    pub fn select_word_at(&mut self, offset: usize) {
        let offset = self.buffer.clamp_offset(offset);
        let text = self.text();
        let start = text[..offset]
            .char_indices()
            .rev()
            .find(|(_, ch)| !is_word(*ch))
            .map_or(0, |(index, ch)| index + ch.len_utf8());
        let end = text[offset..]
            .char_indices()
            .find(|(_, ch)| !is_word(*ch))
            .map_or(text.len(), |(index, _)| offset + index);
        self.anchor = start;
        self.cursor = end;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text().len();
    }
}

pub enum Direction {
    Left,
    Right,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    LineUp,
    LineDown,
    DocumentStart,
    DocumentEnd,
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn word_left(text: &str, cursor: usize) -> usize {
    let mut index = cursor;
    while index > 0 {
        let previous = text[..index]
            .char_indices()
            .next_back()
            .map(|(offset, ch)| (offset, ch))
            .unwrap_or((0, ' '));
        if !is_word(previous.1) {
            break;
        }
        index = previous.0;
    }
    while index > 0 {
        let previous = text[..index]
            .char_indices()
            .next_back()
            .map(|(offset, ch)| (offset, ch))
            .unwrap_or((0, ' '));
        if is_word(previous.1) {
            break;
        }
        index = previous.0;
    }
    index
}

fn word_right(text: &str, cursor: usize) -> usize {
    let mut index = cursor;
    let length = text.len();
    while index < length {
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        if !is_word(ch) {
            break;
        }
        index += ch.len_utf8();
    }
    while index < length {
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        if is_word(ch) {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn floor_boundary(text: &str, byte: usize) -> usize {
    let mut index = byte.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn byte_index(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index)
}

fn char_count(text: &str) -> usize {
    text.chars().count()
}

#[cfg(test)]
#[path = "../../tests/state/editor.rs"]
mod tests;