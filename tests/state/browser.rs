use super::*;

#[test]
fn name_sort_keeps_folders_first_and_preserves_selection_paths() {
    let entry = |name: &str, directory| Entry {
        path: format!("/test/{name}").into(),
        name: name.into(),
        directory,
        bytes: None,
    };
    let mut entries = vec![
        entry("beta.txt", false),
        entry("Zulu", true),
        entry("Alpha", true),
        entry("aardvark.txt", false),
    ];
    let mut selection = Selection::default();
    selection.click(0, false, false);
    selection.click(2, true, false);
    sort_entries(&mut entries, &mut selection, true);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["Zulu", "Alpha", "beta.txt", "aardvark.txt"]
    );
    assert_eq!(
        selection.indices.iter().copied().collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(selection.focus, Some(1));
    assert_eq!(selection.anchor, Some(1));
    sort_entries(&mut entries, &mut selection, false);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Zulu", "aardvark.txt", "beta.txt"]
    );
    assert_eq!(
        selection.indices.iter().copied().collect::<Vec<_>>(),
        [0, 3]
    );
    assert_eq!(selection.primary(), Some(0));
}
