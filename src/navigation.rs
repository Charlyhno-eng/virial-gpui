use crate::location::Location;
use std::path::{Path, PathBuf};

/// History changes are committed only after a directory has been read successfully.
pub struct History {
    paths: Vec<Location>,
    cursor: usize,
}

impl History {
    pub fn new(path: Location) -> Self {
        Self {
            paths: vec![path],
            cursor: 0,
        }
    }

    pub fn back(&self) -> Option<(usize, Location)> {
        self.cursor
            .checked_sub(1)
            .map(|index| (index, self.paths[index].clone()))
    }

    pub fn forward(&self) -> Option<(usize, Location)> {
        let index = self.cursor + 1;
        self.paths.get(index).cloned().map(|path| (index, path))
    }

    pub fn visit(&mut self, path: Location) {
        if self.paths[self.cursor] == path {
            return;
        }
        self.paths.truncate(self.cursor + 1);
        self.paths.push(path);
        self.cursor = self.paths.len() - 1;
    }

    pub fn restore(&mut self, index: usize) {
        if index < self.paths.len() {
            self.cursor = index;
        }
    }
}

pub fn breadcrumbs(path: &Path, home: &Path) -> Vec<(String, PathBuf)> {
    let (mut result, mut current, rest) = if let Ok(rest) = path.strip_prefix(home) {
        (
            vec![("Home".into(), home.to_path_buf())],
            home.to_path_buf(),
            rest,
        )
    } else {
        (
            vec![("File System".into(), PathBuf::from("/"))],
            PathBuf::from("/"),
            path,
        )
    };
    for component in rest.components() {
        if let std::path::Component::Normal(name) = component {
            current.push(name);
            result.push((name.to_string_lossy().into_owned(), current.clone()));
        }
    }
    result
}

#[cfg(test)]
mod tests {
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
}
