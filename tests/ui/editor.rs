use super::*;

// The widget is exercised through the same buffer API the key handler drives,
// so these tests pin the behavior the status bar reports.

#[test]
fn the_status_bar_counts_lines_columns_and_the_modified_flag() {
    let editor = Editor::new("first\nsecond\n", test_format());
    assert_eq!(editor.line_column(), (1, 1));
    assert!(!editor.is_dirty());
}

#[test]
fn typing_reports_the_new_cursor_position() {
    let mut editor = Editor::new("first\nsecond\n", test_format());
    editor.set_cursor(5, false);
    editor.insert("-X");
    assert_eq!(editor.text(), "first-X\nsecond\n");
    assert_eq!(editor.line_column(), (1, 8));
    assert!(editor.is_dirty());
}

#[test]
fn a_save_resets_the_modified_flag_without_touching_the_buffer() {
    let mut editor = Editor::new("hello\n", test_format());
    editor.set_cursor(5, false);
    editor.insert("!");
    assert!(editor.is_dirty());
    editor.mark_saved();
    assert!(!editor.is_dirty());
    assert_eq!(editor.text(), "hello!\n");
}

fn test_format() -> crate::infrastructure::editor_io::DocumentFormat {
    crate::infrastructure::editor_io::DocumentFormat {
        line_ending: crate::infrastructure::editor_io::LineEnding::Lf,
        bom: false,
        latin1_fallback: false,
    }
}