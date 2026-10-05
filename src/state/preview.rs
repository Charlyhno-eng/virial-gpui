//! Selection-driven, bounded previews loaded away from the UI thread.
use super::code_preview::CodePreview;
use crate::infrastructure::image_edit::{ExportFormat, ImageEdit};
use crate::{app::FileManager, domain::models::Entry};
use gpui::{AppContext, Context, Window};
use std::{fs::File, io::Read, path::PathBuf};

pub(crate) enum Preview {
    Loading,
    Folder(usize),
    Image(PathBuf),
    ArchiveImage(crate::infrastructure::archive::Materialized),
    Pdf(crate::infrastructure::archive::Materialized),
    Text(String),
    Code(CodePreview),
    Unavailable,
}

fn read_preview(entry: &Entry, hidden: bool) -> Preview {
    if entry.browsable() {
        return crate::infrastructure::storage::read_directory(&entry.path, hidden)
            .map(|entries| Preview::Folder(entries.len()))
            .unwrap_or(Preview::Unavailable);
    }
    if entry
        .path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return pdf_preview(&entry.path)
            .map(Preview::Pdf)
            .unwrap_or(Preview::Unavailable);
    }
    if crate::infrastructure::archive::is_member(&entry.path) {
        if entry.kind() == "Image" {
            return crate::infrastructure::archive::materialize(&entry.path, 20 * 1024 * 1024)
                .map(Preview::ArchiveImage)
                .unwrap_or(Preview::Unavailable);
        }
        return crate::infrastructure::archive::read_prefix(&entry.path, 64 * 1024)
            .map(|bytes| text_preview(entry, &bytes))
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
    if file.take(64 * 1024).read_to_end(&mut bytes).is_err() {
        return Preview::Unavailable;
    }
    text_preview(entry, &bytes)
}

/// Render a bounded snapshot so local files and ZIP members follow the same limits.
fn pdf_preview(
    path: &std::path::Path,
) -> std::io::Result<crate::infrastructure::archive::Materialized> {
    use std::{
        io,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    const LIMIT: u64 = 20 * 1024 * 1024;
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source.pdf");
    let archived = if crate::infrastructure::archive::is_member(path) {
        Some(crate::infrastructure::archive::materialize(path, LIMIT)?)
    } else {
        None
    };
    let input = File::open(archived.as_ref().map_or(path, |file| file.path.as_path()))?;
    let metadata = input.metadata()?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err(io::Error::other(
            "PDF is too large or is not a regular file",
        ));
    }
    if io::copy(&mut input.take(LIMIT + 1), &mut File::create(&source)?)? > LIMIT {
        return Err(io::Error::other("PDF is too large"));
    }
    let output = directory.path().join("page");
    let mut child = Command::new("pdftoppm")
        .args([
            "-f",
            "1",
            "-l",
            "1",
            "-singlefile",
            "-scale-to",
            "1600",
            "-png",
        ])
        .arg(&source)
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return Err(io::Error::other("Cannot render PDF")),
            Ok(None) if start.elapsed() < Duration::from_secs(10) => {
                std::thread::sleep(Duration::from_millis(25));
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(result.err().unwrap_or_else(|| {
                    io::Error::new(io::ErrorKind::TimedOut, "PDF preview timed out")
                }));
            }
        }
    }
    let path = output.with_extension("png");
    if !path.is_file() {
        return Err(io::Error::other("PDF renderer produced no image"));
    }
    Ok(crate::infrastructure::archive::Materialized {
        path,
        _directory: directory,
    })
}

fn text_preview(entry: &Entry, bytes: &[u8]) -> Preview {
    if bytes.contains(&0) {
        return Preview::Unavailable;
    }
    // A prefix can end in the middle of a UTF-8 character.
    let text = match std::str::from_utf8(bytes) {
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
    pub(crate) fn image_export_dialog(
        &mut self,
        edit: ImageEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy
            || self.dialog.is_some()
            || self.location == crate::domain::location::Location::Trash
            || !matches!(&self.preview, Preview::Image(_) | Preview::ArchiveImage(_))
        {
            return;
        }
        let Some(source) = self.preview_path.clone() else {
            return;
        };
        let name = edit.suggested_name(&source);
        let input = cx.new(|cx| crate::ui::components::input::NameInput::new(name, window, cx));
        self.menu = None;
        self.error = None;
        self.dialog = Some(super::actions::Dialog::ImageExport {
            source,
            edit,
            input,
        });
        cx.notify();
    }

    pub(crate) fn image_export_format(
        &mut self,
        format: ExportFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(super::actions::Dialog::ImageExport { edit, input, .. }) = &mut self.dialog
            && matches!(edit, ImageEdit::Convert(_))
        {
            *edit = ImageEdit::Convert(format);
            let name = PathBuf::from(&input.read(cx).text)
                .with_extension(format.extension())
                .to_string_lossy()
                .into_owned();
            *input = cx.new(|cx| crate::ui::components::input::NameInput::new(name, window, cx));
            self.error = None;
            cx.notify();
        }
    }

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
        self.preview_line = 0;
        self.preview_focused = false;
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
