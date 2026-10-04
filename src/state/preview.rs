//! Selection-driven, bounded previews loaded away from the UI thread.
use super::code_preview::CodePreview;
use crate::{app::FileManager, domain::models::Entry};
use gpui::Context;
use std::{fs::File, io::Read, path::PathBuf};

pub(crate) enum Preview {
    Loading,
    Folder(usize),
    Image(PathBuf),
    Text(String),
    Code(CodePreview),
    Unavailable,
}

fn read_preview(entry: &Entry, hidden: bool) -> Preview {
    if entry.directory {
        return crate::infrastructure::storage::read_directory(&entry.path, hidden)
            .map(|entries| Preview::Folder(entries.len()))
            .unwrap_or(Preview::Unavailable);
    }
    if !std::fs::metadata(&entry.path).is_ok_and(|metadata| metadata.is_file()) {
        return Preview::Unavailable;
    }
    let Ok(file) = File::open(&entry.path) else {
        return Preview::Unavailable;
    };
    // Avoid special files and unbounded image decoding.
    let Ok(metadata) = file.metadata() else {
        return Preview::Unavailable;
    };
    if !metadata.is_file() {
        return Preview::Unavailable;
    }
    if entry.kind() == "Image" {
        return if metadata.len() <= 20 * 1024 * 1024 {
            Preview::Image(entry.path.clone())
        } else {
            Preview::Unavailable
        };
    }
    let mut bytes = Vec::new();
    if file.take(64 * 1024).read_to_end(&mut bytes).is_err() || bytes.contains(&0) {
        return Preview::Unavailable;
    }
    // A prefix can end in the middle of a UTF-8 character.
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(error) if error.error_len().is_none() => {
            std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap_or("")
        }
        Err(_) => return Preview::Unavailable,
    };
    match CodePreview::new(&entry.path, text) {
        Some(code) => Preview::Code(code),
        None => Preview::Text(text.to_owned()),
    }
}

impl FileManager {
    pub(crate) fn sync_preview(&mut self, cx: &mut Context<Self>) {
        let entry = self
            .selection
            .primary()
            .and_then(|index| self.entries.get(index))
            .cloned();
        let path = entry.as_ref().map(|entry| entry.path.clone());
        if path == self.preview_path {
            return;
        }
        self.preview_path = path;
        self.preview_task = None;
        self.preview_expanded = false;
        if entry.is_none() {
            self.details_open = false;
        }
        self.preview = Preview::Loading;
        let Some(entry) = entry else {
            return;
        };
        let hidden = true;
        let read = cx
            .background_executor()
            .spawn(async move { read_preview(&entry, hidden) });
        self.preview_task = Some(cx.spawn(async move |view, cx| {
            let preview = read.await;
            let _ = view.update(cx, |view, cx| {
                view.preview = preview;
                cx.notify();
            });
        }));
    }

    pub(crate) fn close_preview(&mut self, cx: &mut Context<Self>) {
        self.preview_expanded = false;
        self.details_open = false;
        self.selection.clear();
        self.preview_path = None;
        self.preview_task = None;
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/state/preview.rs"]
mod tests;
