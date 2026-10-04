use super::rename_selection_end;

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
