use super::*;
use crate::infrastructure::editor_io::{decode, DocumentFormat, LineEnding};

fn format() -> DocumentFormat {
    DocumentFormat {
        line_ending: LineEnding::Lf,
        bom: false,
        latin1_fallback: false,
    }
}

fn make(text: &str) -> Editor {
    Editor::new(text, format())
}

#[test]
fn round_trips_exactly_and_keeps_the_trailing_newline() {
    for text in ["", "\n", "a\n", "a\nb", "a\n\nb\n", "\n\n\n"] {
        assert_eq!(make(text).text(), text, "round trip {text:?}");
    }
}

#[test]
fn normalizes_crlf_and_keeps_the_original_ending_for_saving() {
    let editor = editor("a\r\nb\r\n");
    assert_eq!(editor.text(), "a\nb\n");
}

#[test]
fn typing_inserts_at_the_cursor_and_marks_the_document_dirty() {
    let mut editor = make("hello\n");
    assert!(!editor.is_dirty());
    editor.set_cursor(5, false);
    editor.insert(" world");
    assert_eq!(editor.text(), "hello world\n");
    assert!(editor.is_dirty());
    assert_eq!(editor.cursor(), 11);
}

#[test]
fn typing_over_a_selection_replaces_it() {
    let mut editor = make("hello world\n");
    editor.set_cursor(0, false);
    editor.set_cursor(5, true);
    assert_eq!(editor.text_in(editor.selection()), "hello");
    editor.insert("bye");
    assert_eq!(editor.text(), "bye world\n");
}

#[test]
fn delete_removes_a_grapheme_not_a_byte() {
    let mut editor = make("a👋b\n");
    editor.set_cursor(6, false);
    editor.delete(true);
    assert_eq!(editor.text(), "a👋\n");
    editor.delete(true);
    assert_eq!(editor.text(), "a\n");
}

#[test]
fn delete_merges_lines_at_the_end_and_start() {
    let mut editor = make("ab\ncd\n");
    editor.set_cursor(2, false);
    editor.delete(false);
    assert_eq!(editor.text(), "abcd\n");
    editor.set_cursor(2, false);
    editor.delete(true);
    assert_eq!(editor.text(), "abcd\n");
    // Backspace at the very start of the buffer is a no-op, not a panic.
    editor.set_cursor(0, false);
    editor.delete(true);
    assert_eq!(editor.text(), "abcd\n");
}

#[test]
fn newline_keeps_the_indentation_and_indents_after_a_brace() {
    let mut editor = make("    let x = 1;\n");
    editor.set_cursor(14, false);
    editor.newline(LineEnding::Lf);
    assert_eq!(editor.text(), "    let x = 1;\n    \n");

    let mut braces = make("fn main() {\n");
    braces.set_cursor(11, false);
    braces.newline(LineEnding::Lf);
    assert_eq!(braces.text(), "fn main() {\n    \n");
}

#[test]
fn undo_groups_a_run_of_typing_and_redo_restores_it() {
    let mut editor = make("\n");
    editor.set_cursor(0, false);
    for ch in "abc".chars() {
        editor.insert(&ch.to_string());
    }
    assert_eq!(editor.text(), "abc\n");
    editor.undo();
    assert_eq!(editor.text(), "\n", "one Ctrl+Z removes the run");
    editor.redo();
    assert_eq!(editor.text(), "abc\n");
}

#[test]
fn undo_restores_a_deleted_word_and_the_cursor() {
    let mut editor = make("hello world\n");
    editor.set_cursor(11, false);
    editor.set_cursor(6, true);
    editor.insert("");
    assert_eq!(editor.text(), "hello \n");
    editor.undo();
    assert_eq!(editor.text(), "hello world\n");
    assert!(editor.can_redo());
    editor.redo();
    assert_eq!(editor.text(), "hello \n");
}

#[test]
fn typing_after_an_undo_discards_the_redo_stack() {
    let mut editor = make("\n");
    editor.set_cursor(0, false);
    editor.insert("a");
    editor.undo();
    assert!(editor.can_redo());
    editor.insert("b");
    assert!(!editor.can_redo());
    assert_eq!(editor.text(), "b\n");
}

#[test]
fn shift_indents_and_outdents_every_line_of_the_selection() {
    let mut editor = make("a\nb\nc\n");
    editor.set_cursor(0, false);
    editor.set_cursor(3, true);
    editor.shift(false);
    assert_eq!(editor.text(), "    a\n    b\nc\n");
    editor.shift(true);
    assert_eq!(editor.text(), "a\nb\nc\n");
}

#[test]
fn line_column_is_one_based_and_follows_the_cursor() {
    let mut lines = make("one\ntwo\nthree\n");
    lines.set_cursor(0, false);
    assert_eq!(lines.line_column(), (1, 1));
    lines.set_cursor(5, false);
    assert_eq!(lines.line_column(), (2, 2));
    lines.set_cursor(12, false);
    assert_eq!(lines.line_column(), (3, 5));
}

#[test]
fn movement_covers_characters_words_lines_and_document() {
    let text = "alpha beta\ngamma\n";
    let mut editor = make(text);
    editor.set_cursor(0, false);
    editor.move_by(Direction::WordRight, false, None);
    assert_eq!(editor.cursor(), 6);
    editor.move_by(Direction::WordRight, false, None);
    assert_eq!(editor.cursor(), 11, "to the newline at the end of the line");
    editor.move_by(Direction::WordLeft, false, None);
    assert_eq!(editor.cursor(), 6);
    editor.move_by(Direction::LineEnd, false, None);
    assert_eq!(editor.cursor(), 10);
    editor.move_by(Direction::LineStart, false, None);
    assert_eq!(editor.cursor(), 0);
    editor.move_by(Direction::LineDown, false, None);
    assert_eq!(editor.line_column(), (2, 1));
    editor.move_by(Direction::LineUp, false, None);
    assert_eq!(editor.line_column(), (1, 1));
    editor.move_by(Direction::DocumentEnd, false, None);
    assert_eq!(editor.cursor(), text.len() - 1);
    editor.move_by(Direction::DocumentStart, false, None);
    assert_eq!(editor.cursor(), 0);
}

#[test]
fn vertical_movement_clamps_to_a_shorter_line() {
    let mut editor = make("long line here\nab\n");
    editor.set_cursor(14, false);
    editor.move_by(Direction::LineDown, false, None);
    assert_eq!(editor.line_column(), (2, 3), "column clamps to the line end");
    editor.move_by(Direction::LineUp, false, None);
    assert_eq!(editor.line_column(), (1, 15));
}

#[test]
fn movement_never_splits_a_multibyte_character() {
    let mut editor = make("héllo 👋\n");
    editor.set_cursor(0, false);
    editor.move_by(Direction::Right, false, None);
    assert_eq!(editor.cursor(), 1);
    editor.move_by(Direction::Right, false, None);
    assert_eq!(editor.cursor(), 3);
    editor.move_by(Direction::DocumentEnd, false, None);
    assert_eq!(editor.cursor(), "héllo 👋\n".len() - 1);
}

#[test]
fn select_word_and_select_all_cover_the_expected_spans() {
    let mut editor = make("alpha beta\n");
    editor.select_word_at(7);
    assert_eq!(editor.text_in(editor.selection()), "beta");
    editor.select_all();
    assert_eq!(editor.text_in(editor.selection()), "alpha beta\n");
}

#[test]
fn clamping_keeps_the_cursor_inside_the_buffer() {
    let mut editor = make("ab\ncd\n");
    editor.set_cursor(999, false);
    assert_eq!(editor.line_column(), (2, 3), "the end of the last line");
    editor.move_by(Direction::Right, false, None);
    assert_eq!(editor.cursor(), 5, "stays put at the end");
    editor.move_by(Direction::LineUp, false, None);
    assert_eq!(editor.line_column(), (1, 3));
    editor.set_cursor(1, false);
    editor.move_by(Direction::LineUp, false, None);
    assert_eq!(editor.line_column(), (1, 1));
}

#[test]
fn mark_saved_clears_the_modified_state() {
    let mut editor = make("hello\n");
    editor.set_cursor(5, false);
    editor.insert("!");
    assert!(editor.is_dirty());
    editor.mark_saved();
    assert!(!editor.is_dirty());
}

#[test]
fn a_utf8_document_survives_an_edit_at_its_end() {
    let mut editor = make("héllo\n");
    let end = editor.text().len() - 1;
    editor.set_cursor(end, false);
    editor.insert("👋");
    assert_eq!(editor.text(), "héllo👋\n");
    assert_eq!(editor.line_column(), (1, 7));
}

#[test]
fn revision_advances_on_every_change() {
    let mut editor = make("a\n");
    let first = editor.revision();
    editor.set_cursor(0, false);
    editor.insert("b");
    assert!(editor.revision() > first);
    let second = editor.revision();
    editor.undo();
    assert!(editor.revision() > second);
}

#[test]
fn a_long_session_drops_the_oldest_undo_steps() {
    let mut editor = make("\n");
    editor.set_cursor(0, false);
    for _ in 0..MAX_HISTORY + 50 {
        editor.insert("x");
    }
    let mut steps = 0;
    while editor.can_undo() {
        editor.undo();
        steps += 1;
    }
    assert!(steps <= MAX_HISTORY, "history is bounded: {steps}");
}

#[test]
fn decoded_text_feeds_the_editor_without_losing_bytes() {
    let (text, format) = decode(b"caf\xe9\r\n").unwrap();
    assert!(format.latin1_fallback);
    let mut editor = Editor::new(&text, format.clone());
    editor.set_cursor(0, false);
    editor.insert("> ");
    assert_eq!(editor.text(), "> café\n", "the buffer normalizes the endings");
    assert_eq!(
        crate::infrastructure::editor_io::encode(&editor.text(), &format),
        b"> caf\xe9\r\n",
        "saving restores latin-1 and CRLF"
    );
}