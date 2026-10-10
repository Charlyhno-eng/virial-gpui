//! Opening, saving and closing the text editor from the file manager.
//!
//! Reading and writing happen on a background thread; only the resulting
//! buffer or error reaches the UI thread.

use crate::app::FileManager;
use crate::infrastructure::editor_io;
use crate::state::editor::Editor;
use crate::ui::components::editor::CodeEditor;
use gpui::{App, AppContext, Context, Window};
use std::path::PathBuf;

/// Largest file the editor opens. Beyond this the read-only preview is used.
const MAX_EDITABLE: u64 = 8 * 1024 * 1024;

impl FileManager {
    /// Load a file into the editor, replacing any preview content.
    pub(crate) fn open_editor(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.language;
        let read = cx.background_executor().spawn(async move {
            let metadata = std::fs::metadata(&path).ok()?;
            if !metadata.is_file() || metadata.len() > MAX_EDITABLE {
                return None;
            }
            let bytes = std::fs::read(&path).ok()?;
            let (text, format) = editor_io::decode(&bytes)?;
            Some((text, format, editor_io::fingerprint(&bytes, metadata.modified().ok())))
        });
        cx.spawn(async move |view, cx| {
            let loaded = read.await;
            let _ = view.update(cx, |view, window, cx| {
                match loaded {
                    Some((text, format, fingerprint)) => {
                        view.editor_disk = Some(fingerprint);
                        view.attach_editor(
                            Editor::new(&text, format),
                            path,
                            window,
                            cx,
                        );
                    }
                    None => {
                        view.error = Some(
                            language
                                .text("Cannot open this file in the editor")
                                .to_string(),
                        );
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// Put a ready buffer on screen, replacing whatever the preview showed.
    pub(crate) fn attach_editor(
        &mut self,
        editor: Editor,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut view = cx.new(|cx| CodeEditor::new(editor, path, window, cx));
        view.update(cx, |view, _| view.set_language(self.language));
        self.editor = Some(view.clone());
        self.preview = crate::state::preview::Preview::Editing;
        self.error = None;
        cx.notify();
        if let Some(view) = &self.editor {
            view.update(cx, |view, _| view.focus(window));
        }
    }

    /// Close the editor, returning to the read-only preview of the same file.
    pub(crate) fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_is_dirty(cx) {
            // Unsaved work is never dropped silently: the file manager keeps
            // the editor open and says why.
            self.error = Some(self.language.text("Save the file before closing it").into());
            cx.notify();
            return;
        }
        self.editor = None;
        self.editor_disk = None;
        self.editor_task = None;
        self.focus.focus(window);
        self.sync_preview(cx);
        cx.notify();
    }

    fn editor_is_dirty(&self, cx: &App) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).is_dirty())
    }

    /// Write the buffer to disk: Ctrl+S.
    pub(crate) fn save_editor(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.editor.clone() else {
            return;
        };
        if self.busy || !view.read(cx).is_dirty() {
            return;
        }
        let path = view.read(cx).path().to_path_buf();
        let text = view.read(cx).text();
        let format = view.read(cx).format();
        let known = self.editor_disk;
        let language = self.language;

        // Re-read the file first: saving over someone else's edit loses work.
        let probe = path.clone();
        let check = cx.background_executor().spawn(async move {
            let metadata = std::fs::metadata(&probe).ok()?;
            let bytes = std::fs::read(&probe).ok()?;
            Some((
                editor_io::fingerprint(&bytes, metadata.modified().ok()),
                bytes.len() as u64,
            ))
        });
        cx.spawn(async move |view, cx| {
            let current = check.await;
            let _ = view.update(cx, |view, cx| {
                if let (Some(known), Some((disk, _))) = (known, current)
                    && disk != known
                {
                    view.error = Some(
                        language
                            .text("The file changed on disk since it was opened")
                            .to_string(),
                    );
                    cx.notify();
                    return;
                }
                view.editor_task = Some(cx.spawn(async move |view, cx| {
                    let write = cx
                        .background_executor()
                        .spawn(async move { editor_io::write_atomic(&path, &editor_io::encode(&text, &format)) });
                    let result = write.await;
                    let _ = view.update(cx, |view, cx| {
                        match result {
                            Ok(()) => {
                                if let Some(editor) = &view.editor {
                                    editor.update(cx, |editor, _| editor.mark_saved());
                                }
                                view.error = None;
                                view.entry_modified.insert(
                                    path,
                                    chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
                                );
                            }
                            Err(error) => {
                                view.error = Some(
                                    format!(
                                        "{}: {error}",
                                        view.language.text("Cannot save the file")
                                    ),
                                );
                            }
                        }
                        cx.notify();
                    });
                }));
                cx.notify();
            });
        })
        .detach();
    }
}