use super::*;

#[test]
fn history_supports_back_forward_and_branching_without_duplicate_refreshes() {
    let mut history = History::new("/home".into());
    assert!(history.back().is_none());
    history.visit("/home/docs".into());
    history.visit("/home/docs".into());
    history.visit("/tmp".into());
    let (index, path) = history.back().unwrap();
    assert_eq!(path, Location::from("/home/docs"));
    history.restore(index);
    assert_eq!(history.forward().unwrap().1, Location::from("/tmp"));
    history.visit("/etc".into());
    assert!(history.forward().is_none());
    assert_eq!(history.back().unwrap().1, Location::from("/home/docs"));
}

#[test]
fn history_includes_virtual_locations() {
    let mut history = History::new("/home".into());
    history.visit(Location::Recent);
    history.visit(Location::Network);
    let (index, location) = history.back().unwrap();
    assert_eq!(location, Location::Recent);
    history.restore(index);
    assert_eq!(history.forward().unwrap().1, Location::Network);
}

#[test]
fn breadcrumbs_preserve_real_paths_and_only_shorten_the_home_prefix() {
    let home = Path::new("/home/user");
    let crumbs = breadcrumbs(Path::new("/home/user/Documents/Work"), home);
    assert_eq!(crumbs[0], ("Home".into(), home.into()));
    assert_eq!(crumbs[2].1, PathBuf::from("/home/user/Documents/Work"));
    assert_eq!(breadcrumbs(Path::new("/"), home).len(), 1);
    assert_eq!(
        breadcrumbs(Path::new("/home/username"), home)[0].0,
        "File System"
    );
}
