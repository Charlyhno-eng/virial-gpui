//! Installed desktop applications; GIO handles Exec expansion and launching.
use crate::ui::i18n::Language;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct Application {
    pub name: String,
    pub desktop: PathBuf,
}

fn visit(directory: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>) {
    let Ok(items) = fs::read_dir(directory) else {
        return;
    };
    for item in items.flatten() {
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        if item.file_type().is_ok_and(|kind| kind.is_dir()) {
            visit(&path, &format!("{prefix}{name}-"), files);
        } else if path.extension().is_some_and(|ext| ext == "desktop") {
            files.push((format!("{prefix}{name}"), path));
        }
    }
}

fn parse(text: &str, language: Language) -> Option<String> {
    let mut fields = HashMap::new();
    let mut active = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            active = line == "[Desktop Entry]";
        } else if active && !line.starts_with('#') && line.contains('=') {
            let (key, value) = line.split_once('=').expect("contains '=' checked above");
            fields.insert(key.trim(), value.trim());
        }
    }
    if fields.get("Type") != Some(&"Application")
        || fields.get("Hidden") == Some(&"true")
        || fields.get("NoDisplay") == Some(&"true")
        || (!fields.contains_key("Exec") && fields.get("DBusActivatable") != Some(&"true"))
    {
        return None;
    }
    let localized = if language == Language::French {
        "Name[fr]"
    } else {
        "Name[en]"
    };
    fields
        .get(localized)
        .or_else(|| fields.get("Name"))
        .map(|name| name.replace("\\s", " ").replace("\\\\", "\\"))
}

pub fn installed(home: &Path, language: Language) -> Vec<Application> {
    let data = crate::infrastructure::recent::data_home(home);
    let mut directories = vec![data];
    directories.extend(
        std::env::var_os("XDG_DATA_DIRS")
            .filter(|value| !value.is_empty())
            .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
            .unwrap_or_else(|| vec!["/usr/local/share".into(), "/usr/share".into()]),
    );
    let mut seen = HashSet::new();
    let mut applications = Vec::new();
    for directory in directories.into_iter().filter(|path| path.is_absolute()) {
        let mut files = Vec::new();
        visit(&directory.join("applications"), "", &mut files);
        for (id, desktop) in files {
            if !seen.insert(id) {
                continue;
            }
            if let Some(name) = fs::read_to_string(&desktop)
                .ok()
                .and_then(|text| parse(&text, language))
            {
                applications.push(Application { name, desktop });
            }
        }
    }
    applications.sort_by_key(|app| app.name.to_lowercase());
    applications
}
#[cfg(test)]
#[path = "../../../tests/platform/linux/applications.rs"]
mod tests;
