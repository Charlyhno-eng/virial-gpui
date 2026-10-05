use super::{rename_selection_end, word_range};

#[test]
fn control_backspace_deletes_the_previous_word_and_separator() {
    let text = "one two";
    assert_eq!(&text[word_range(text, text.len(), true)], "two");
    assert_eq!(&text[word_range(text, 4, true)], "one ");
}

#[test]
fn control_delete_deletes_the_next_word_and_separator() {
    let text = "one two";
    assert_eq!(&text[word_range(text, 0, false)], "one ");
    assert_eq!(&text[word_range(text, 4, false)], "two");
}

#[test]
fn control_word_deletion_preserves_unicode_graphemes() {
    let text = "café 🦀 docs";
    assert_eq!(&text[word_range(text, 10, true)], "🦀");
    assert_eq!(&text[word_range(text, 0, false)], "café ");
}

#[test]
fn rename_selects_the_name_without_the_final_extension() {
    for (name, selected) in [
        ("report.pdf", "report"),
        ("archive.tar.gz", "archive.tar"),
        ("README", "README"),
        (".gitignore", ".gitignore"),
        (".config.json", ".config"),
        ("résumé🦀.txt", "résumé🦀"),
        ("name.", "name"),
    ] {
        let end = rename_selection_end(name, false);
        assert_eq!(&name[..end], selected);
    }
}

#[test]
fn rename_selects_the_entire_folder_name() {
    for name in ["folder", "folder.d", ".config", "données.txt"] {
        assert_eq!(rename_selection_end(name, true), name.len());
    }
}
