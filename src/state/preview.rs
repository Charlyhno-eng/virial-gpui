//! Selection-driven, bounded previews loaded away from the UI thread.
use super::code_preview::CodePreview;
use crate::infrastructure::image_edit::{ExportFormat, ImageEdit};
use crate::{app::FileManager, domain::models::Entry};
use gpui::{AppContext, Context, Window};
use std::{fs::File, io::Read, path::PathBuf};

pub(crate) enum Preview {
    Loading,
    Folder(Vec<Entry>),
    Image(PathBuf, Option<ImageMetadata>),
    ArchiveImage(
        crate::infrastructure::archive::Materialized,
        Option<ImageMetadata>,
    ),
    Pdf(crate::infrastructure::archive::Materialized),
    Media(crate::infrastructure::media::Media),
    Text(String),
    Code(CodePreview),
    /// A formatted Markdown document, parsed on the worker.
    Markdown(super::markdown::Document),
    /// An HTML page, parsed on the worker; `bool` = rendered page view.
    Html(super::html::Document, bool),
    /// The file is open in the editor rather than only previewed.
    Editing,
    Unavailable,
}

/// A retained parent column, including its independent scroll position.
pub(crate) struct PreviewColumn {
    pub entry: Entry,
    pub entries: Vec<Entry>,
    pub scroll: gpui::UniformListScrollHandle,
    pub selection: super::selection::Selection,
}

impl PreviewColumn {
    pub fn branch(columns: &mut Vec<Self>, index: usize) {
        columns.truncate(index + 1);
    }
}

pub(crate) const MAX_PREVIEW_COLUMNS: usize = 4;

/// Global column zero is the main file list; a file takes two width units.
pub(crate) fn visible_column_start(parents: usize, file: bool) -> usize {
    (parents + 2 + usize::from(file)).saturating_sub(MAX_PREVIEW_COLUMNS)
}

fn keyboard_preview_entry(
    selection: &mut super::selection::Selection,
    entries: &[Entry],
    scroll: &gpui::UniformListScrollHandle,
    key: &str,
) -> Option<Entry> {
    let page =
        (f32::from(scroll.0.borrow().base_handle.bounds().size.height) / 36.).floor() as usize;
    let index = selection.keyboard_target(key, entries.len(), page)?;
    selection.click(index, false, false);
    scroll.scroll_to_item(index, gpui::ScrollStrategy::Center);
    entries.get(index).cloned()
}

/// Header-only metadata: no pixel buffer is decoded for the details panel.
pub(crate) struct ImageMetadata {
    pub format: String,
    pub width: u32,
    pub height: u32,
}

fn image_metadata(path: &std::path::Path) -> Option<ImageMetadata> {
    let reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let format = format!("{:?}", reader.format()?);
    let (width, height) = reader.into_dimensions().ok()?;
    Some(ImageMetadata {
        format,
        width,
        height,
    })
}

impl Preview {
    pub(crate) fn image_metadata(&self) -> Option<&ImageMetadata> {
        match self {
            Self::Image(_, metadata) | Self::ArchiveImage(_, metadata) => metadata.as_ref(),
            _ => None,
        }
    }

    /// The syntax-colored source of the previewed document, whichever format
    /// it is. `None` for previews that have no source to show.
    pub(crate) fn source_lines(&self) -> Option<&CodePreview> {
        match self {
            Self::Code(code) => Some(code),
            Self::Html(document, _) => Some(&document.source),
            _ => None,
        }
    }

    /// How many display lines the previewed document has in its current view.
    pub(crate) fn line_count(&self) -> usize {
        match self {
            Self::Text(text) => text.split('\n').count(),
            Self::Code(code) => code.lines.len(),
            Self::Markdown(document) => document.rendered.len(),
            Self::Html(document, rendered) => {
                if *rendered {
                    document.rendered.len()
                } else {
                    document.source.lines.len()
                }
            }
            _ => 1,
        }
        .max(1)
    }
}

fn read_preview(entry: &Entry, hidden: bool) -> Preview {
    if entry.browsable() {
        return crate::infrastructure::storage::read_directory(&entry.path, hidden)
            .map(Preview::Folder)
            .unwrap_or(Preview::Unavailable);
    }
    if matches!(entry.kind(), "Audio" | "Video") {
        return media_preview(entry).unwrap_or(Preview::Unavailable);
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
                .map(|image| {
                    let metadata = image_metadata(&image.path);
                    Preview::ArchiveImage(image, metadata)
                })
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
            Preview::Image(entry.path.clone(), image_metadata(&entry.path))
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

/// Local media streams directly; ZIP extraction is bounded to avoid filling disk.
fn media_preview(entry: &Entry) -> Option<Preview> {
    let archived = if crate::infrastructure::archive::is_member(&entry.path) {
        Some(crate::infrastructure::archive::materialize(&entry.path, 512 * 1024 * 1024).ok()?)
    } else {
        None
    };
    let path = archived.as_ref().map_or(&entry.path, |file| &file.path);
    if !std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return None;
    }
    Some(Preview::Media(crate::infrastructure::media::Media::new(
        path.clone(),
        entry.kind() == "Video",
        archived,
    )))
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
    // Parse formatted documents on the worker; HTML also retains its source view.
    let lower = entry
        .path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match lower.as_deref() {
        Some("md" | "markdown") => {
            return Preview::Markdown(super::markdown::Document::new(&entry.path, text));
        }
        Some("html" | "htm" | "xhtml") => {
            return Preview::Html(super::html::Document::new(&entry.path, text), true);
        }
        _ => {}
    }
    match CodePreview::new(&entry.path, text) {
        Some(code) => Preview::Code(code),
        None => Preview::Text(text.to_owned()),
    }
}

impl FileManager {
    pub(crate) fn preview_is_file(&self) -> bool {
        self.preview_entry
            .as_ref()
            .is_some_and(|entry| !entry.browsable())
    }

    pub(crate) fn visible_preview_column_start(&self) -> usize {
        visible_column_start(self.preview_history.len(), self.preview_is_file())
    }

    /// Move within the focused directory without changing the main location.
    pub(crate) fn preview_keyboard_select(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let column = self.preview_active_column;
        if !self.details_open || column == 0 {
            return false;
        }
        let (selection, entries, scroll) =
            if let Some(parent) = self.preview_history.get_mut(column - 1) {
                (&mut parent.selection, &parent.entries, &parent.scroll)
            } else if let Preview::Folder(entries) = &self.preview {
                (
                    &mut self.preview_selection,
                    entries,
                    &self.code_preview_scroll,
                )
            } else {
                // An empty, loading or unsupported preview must never steer the main list.
                return true;
            };
        if let Some(entry) = keyboard_preview_entry(selection, entries, scroll, key) {
            if column <= self.preview_history.len() {
                self.preview_column_child(column - 1, entry, cx);
            } else {
                self.preview_child(entry, cx);
            }
        }
        true
    }

    pub(crate) fn preview_keyboard_right(&mut self, cx: &mut Context<Self>) {
        if !self.details_open || self.preview_active_column == 0 {
            if self.selection.primary().is_some() {
                self.show_preview(cx);
                self.preview_active_column = 1;
                self.preview_focused = self.preview_history.is_empty() && self.preview_is_file();
                cx.notify();
            }
            return;
        }
        if self.preview_active_column <= self.preview_history.len() {
            self.preview_active_column += 1;
        } else if let Preview::Folder(entries) = &self.preview {
            let index = self.preview_selection.primary().unwrap_or(0);
            if let Some(entry) = entries.get(index).cloned() {
                self.preview_child(entry, cx);
                self.preview_active_column += 1;
            }
        }
        self.preview_focused =
            self.preview_active_column == self.preview_history.len() + 1 && self.preview_is_file();
        cx.notify();
    }

    pub(crate) fn preview_keyboard_left(&mut self, cx: &mut Context<Self>) {
        if !self.details_open || self.preview_active_column == 0 {
            if self.details_open {
                self.close_preview(cx);
            } else {
                self.up(cx);
            }
            return;
        }
        let parent = self.preview_active_column - 1;
        if parent == 0 {
            // Retain the main selection so another Right immediately reopens it.
            let selection = self.selection.clone();
            self.close_preview(cx);
            self.selection = selection;
        } else {
            self.preview_history.truncate(parent);
            self.preview_back(cx);
        }
        self.preview_active_column = parent;
        self.preview_focused = false;
        cx.notify();
    }

    pub(crate) fn image_export_dialog(
        &mut self,
        edit: ImageEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy
            || self.dialog.is_some()
            || self.location == crate::domain::location::Location::Trash
            || !matches!(
                &self.preview,
                Preview::Image(..) | Preview::ArchiveImage(..)
            )
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
        // Do not read, decode or keep temporary files for a hidden preview.
        let entry = self
            .details_open
            .then(|| {
                self.selection
                    .primary()
                    .and_then(|index| self.entries.get(index))
            })
            .flatten();
        let path = entry.map(|entry| entry.path.clone());
        if path == self.preview_root && (path.is_none() || self.preview_path.is_some()) {
            return;
        }
        let entry = entry.cloned();
        self.preview_root = path;
        self.preview_history.clear();
        self.preview_active_column = 0;
        self.preview_column_widths.clear();
        self.preview_columns_scroll = gpui::ScrollHandle::new();
        self.load_preview(entry, cx);
    }

    pub(crate) fn preview_child(&mut self, entry: Entry, cx: &mut Context<Self>) {
        self.preview_active_column = self.preview_history.len() + 1;
        if let Some(current) = self.preview_entry.clone()
            && matches!(self.preview, Preview::Folder(_))
        {
            let Preview::Folder(entries) = std::mem::replace(&mut self.preview, Preview::Loading)
            else {
                unreachable!()
            };
            if let Some(index) = entries.iter().position(|child| child.path == entry.path) {
                self.preview_selection.click(index, false, false);
            }
            self.preview_history.push(PreviewColumn {
                entry: current,
                entries,
                scroll: self.code_preview_scroll.clone(),
                selection: std::mem::take(&mut self.preview_selection),
            });
        }
        self.open_preview_child(entry, cx);
    }

    pub(crate) fn preview_column_child(
        &mut self,
        index: usize,
        entry: Entry,
        cx: &mut Context<Self>,
    ) {
        if index >= self.preview_history.len() {
            return;
        }
        self.preview_active_column = index + 1;
        if let Some(child) = self.preview_history[index]
            .entries
            .iter()
            .position(|child| child.path == entry.path)
        {
            self.preview_history[index]
                .selection
                .click(child, false, false);
        }
        PreviewColumn::branch(&mut self.preview_history, index);
        self.open_preview_child(entry, cx);
    }

    fn open_preview_child(&mut self, entry: Entry, cx: &mut Context<Self>) {
        self.cancel_pending_preview();
        let expanded = self.preview_expanded;
        let changed_kind = self.preview_is_file() == entry.browsable();
        self.load_preview(Some(entry), cx);
        self.preview_expanded = expanded;
        self.preview_column_widths
            .resize(self.preview_history.len() + 2, None);
        if changed_kind {
            self.preview_column_widths[self.preview_history.len() + 1] = None;
        }
        self.preview_columns_scroll
            .scroll_to_item(self.preview_history.len() + 1 - self.visible_preview_column_start());
    }

    pub(crate) fn preview_back(&mut self, cx: &mut Context<Self>) {
        if let Some(column) = self.preview_history.pop() {
            self.cancel_pending_preview();
            let expanded = self.preview_expanded;
            self.load_preview(Some(column.entry), cx);
            // Show the cached parent immediately and retain its scroll position.
            self.preview = Preview::Folder(column.entries);
            self.code_preview_scroll = column.scroll;
            self.preview_selection = column.selection;
            self.preview_active_column = self
                .preview_active_column
                .min(self.preview_history.len() + 1);
            self.preview_expanded = expanded;
            self.preview_column_widths
                .truncate(self.preview_history.len() + 2);
            self.preview_columns_scroll.scroll_to_item(
                self.preview_history.len() + 1 - self.visible_preview_column_start(),
            );
        }
    }

    pub(crate) fn close_preview_column(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == 0 {
            self.close_preview(cx);
        } else {
            self.preview_history.truncate(index);
            self.preview_back(cx);
        }
    }

    fn load_preview(&mut self, entry: Option<Entry>, cx: &mut Context<Self>) {
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let generation = self.preview_generation;
        self.preview_path = entry.as_ref().map(|entry| entry.path.clone());
        self.preview_entry = entry.clone();
        self.preview_selection.clear();
        self.preview_media_updates = None;
        // A cache scoped to the current preview releases decoded pixels and GPU
        // textures when switching files, instead of retaining every opened image.
        self.preview_image_cache = gpui::RetainAllImageCache::new(cx);
        self.preview_line = 0;
        self.code_preview_scroll = gpui::UniformListScrollHandle::new();
        self.preview_focused = false;
        self.preview_task = None;
        self.preview_expanded = false;
        if entry.is_none() {
            self.details_open = false;
        }
        self.preview = Preview::Loading;
        let Some(entry) = entry else {
            self.preview = Preview::Unavailable;
            return;
        };
        // Folder previews list immediate children only: recursive size scans
        // compete with previews and compression for disk access.
        let hidden = true;
        let remote = self.location.remote().cloned();
        let store = self.ssh.store.clone();
        let runtime = self.ssh.runtime.handle().clone();
        let read = cx.background_executor().spawn(async move {
            if let Some(host) = remote {
                let preview = if entry.directory {
                    store
                        .browse(&host, &entry.path, Some(runtime))
                        .await
                        .map(|mut entries| {
                            super::browser::sort_by_name(&mut entries);
                            Preview::Folder(entries)
                        })
                        .unwrap_or(Preview::Unavailable)
                } else {
                    Preview::Unavailable
                };
                return preview;
            }
            read_preview(&entry, hidden)
        });
        cx.notify();
        self.preview_task = Some(cx.spawn(async move |view, cx| {
            let preview = read.await;
            let _ = view.update(cx, |view, cx| {
                if view.preview_generation != generation || !view.details_open {
                    return;
                }
                view.preview = preview;
                if matches!(view.preview, Preview::Media(_)) {
                    view.monitor_preview_media(cx);
                }
                cx.notify();
            });
        }));
    }

    fn monitor_preview_media(&mut self, cx: &mut Context<Self>) {
        self.preview_media_updates = Some(cx.spawn(async move |view, cx| {
            let mut previous = None;
            let mut delay = std::time::Duration::from_millis(100);
            loop {
                cx.background_executor().timer(delay).await;
                let Ok(Some(snapshot)) = view.update(cx, |view, cx| {
                    let Preview::Media(media) = &view.preview else {
                        return None;
                    };
                    if !view.details_open {
                        return None;
                    }
                    let snapshot = media.snapshot();
                    if previous
                        .as_ref()
                        .is_none_or(|last| snapshot.changed_since(last))
                    {
                        cx.notify();
                    }
                    Some((snapshot, media.video))
                }) else {
                    break;
                };
                let (snapshot, video) = snapshot;
                delay = std::time::Duration::from_millis(
                    if video && snapshot.ready && !snapshot.paused {
                        16
                    } else {
                        100
                    },
                );
                if snapshot.failed {
                    break;
                }
                previous = Some(snapshot);
            }
        }));
    }

    pub(crate) fn close_preview(&mut self, cx: &mut Context<Self>) {
        self.cancel_pending_preview();
        self.preview_expanded = false;
        self.details_open = false;
        self.selection.clear();
        self.preview_path = None;
        self.preview_root = None;
        self.preview_entry = None;
        self.preview_history.clear();
        self.preview_active_column = 0;
        self.preview_selection.clear();
        self.preview_generation = self.preview_generation.wrapping_add(1);
        self.preview_task = None;
        self.preview = Preview::Unavailable;
        self.preview_media_updates = None;
        self.preview_image_cache = gpui::RetainAllImageCache::new(cx);
        cx.notify();
    }
}

#[cfg(all(test, unix))]
#[path = "../../tests/state/preview.rs"]
mod tests;
