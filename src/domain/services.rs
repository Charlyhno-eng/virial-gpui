use crate::domain::location::Location;
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
#[path = "../../tests/domain/services.rs"]
mod tests;
