//! The editable code view: gutter, caret, selection and status bar.
//!
//! Coloring is reused from the read-only code preview, so a file looks the same
//! whether it is being viewed or edited. Each visible line shapes its own text,
//! which gives the caret, the selection and a mouse click one shared geometry.

use crate::state::code_preview::{CodeLine, CodePreview};
use crate::state::editor::{Direction, Editor};
use crate::ui::i18n::Language;
use crate::ui::theme::*;
use gpui::{prelude::*, *};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Height of one text line, in pixels.
pub(crate) const LINE_HEIGHT: f32 = 18.;
/// Gutter width, wide enough for four digits and their padding.
const GUTTER_WIDTH: f32 = 44.;
/// Caret blink period.
const BLINK: std::time::Duration = std::time::Duration::from_millis(530);

/// Geometry of one painted line, used to turn a click into a text offset.
struct PaintedRow {
    line: usize,
    /// Top edge of the line, in window coordinates.
    top: Pixels,
    /// Left edge of the text, right of the gutter.
    text_left: Pixels,
    shape: ShapedLine,
}

pub struct CodeEditor {
    editor: Editor,
    focus: FocusHandle,
    scroll: gpui::UniformListScrollHandle,
    /// Coloring for the current text, rebuilt when the revision advances.
    highlighted: Option<(u64, Arc<CodePreview>)>,
    language: Language,
    path: std::path::PathBuf,
    /// Flipped by the blink timer; the caret is painted while true.
        caret_visible: Arc<AtomicBool>,
        blink: Option<Task<()>>,
        /// Byte range of the IME preedit, which the platform underlines.
        marked: Option<Range<usize>>,
    /// Lines painted last frame, in paint order.
    rows: Vec<PaintedRow>,
    dragging: bool,
    /// Bumped per frame so a rebuild gets a fresh uniform-list identity.
    generation: usize,
}

impl CodeEditor {
    pub fn new(
        editor: Editor,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        Self {
            editor,
            focus,
            scroll: gpui::UniformListScrollHandle::new(),
            highlighted: None,
            language: Language::English,
            path,
            caret_visible: Arc::new(AtomicBool::new(true)),
            blink: None,
            marked: None,
            rows: Vec::new(),
            dragging: false,
            generation: 0,
        }
    }

    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    pub fn focus(&self, window: &mut Window) {
        self.focus.focus(window);
        self.caret_visible.store(true, Ordering::Relaxed);
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn is_dirty(&self) -> bool {
        self.editor.is_dirty()
    }

    pub fn can_undo(&self) -> bool {
        self.editor.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.editor.can_redo()
    }

    pub fn text(&self) -> String {
        self.editor.text()
    }

    pub fn format(&self) -> crate::infrastructure::editor_io::DocumentFormat {
        self.editor.buffer().format.clone()
    }

    /// After a successful write the buffer is the file again.
    pub fn mark_saved(&mut self) {
        self.editor.mark_saved();
    }

    pub fn line_column(&self) -> (usize, usize) {
        self.editor.line_column()
    }

    /// Coloring for the current revision, computed once and reused.
    fn highlighted_preview(&mut self, cx: &mut Context<Self>) -> Option<Arc<CodePreview>> {
        let revision = self.editor.revision();
        if let Some((seen, code)) = &self.highlighted
            && *seen == revision
        {
            return Some(code.clone());
        }
        let code = CodePreview::new(&self.path, &self.editor.text())?;
        let code = Arc::new(code);
        self.highlighted = Some((revision, code.clone()));
        let _ = cx;
        Some(code)
    }

    /// Keys an IDE owns while the view has focus. Returns true when consumed,
    /// so the file list behind never sees them.
    pub fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let extend = modifiers.shift;
        let eol = self.editor.line_ending();
        let page = self.page();
        let word = modifiers.control || modifiers.alt;
        match key {
            "tab" if modifiers.shift => self.editor.shift(true),
            "tab" => self.editor.shift(false),
            "left" if word => self.editor.move_by(Direction::WordLeft, extend, None),
            "right" if word => self.editor.move_by(Direction::WordRight, extend, None),
            "up" if modifiers.control => self.editor.move_by(Direction::LineUp, extend, None),
            "down" if modifiers.control => self.editor.move_by(Direction::LineDown, extend, None),
            "left" => self.editor.move_by(Direction::Left, extend, None),
            "right" => self.editor.move_by(Direction::Right, extend, None),
            "up" => self.editor.move_by(Direction::LineUp, extend, None),
            "down" => self.editor.move_by(Direction::LineDown, extend, None),
            "home" if modifiers.control => self.editor.move_by(Direction::DocumentStart, extend, None),
            "end" if modifiers.control => self.editor.move_by(Direction::DocumentEnd, extend, None),
            "home" => self.editor.move_by(Direction::LineStart, extend, None),
            "end" => self.editor.move_by(Direction::LineEnd, extend, None),
            "pageup" => self.editor.move_by(Direction::LineUp, extend, Some(page)),
            "pagedown" => self.editor.move_by(Direction::LineDown, extend, Some(page)),
            "backspace" => self.editor.delete(true),
            "delete" => self.editor.delete(false),
            "enter" => {
                self.editor.newline(eol);
                self.scroll_to_cursor();
            }
            "a" if modifiers.control => self.editor.select_all(),
            "z" if modifiers.control && modifiers.shift => self.editor.redo(),
            "z" if modifiers.control => self.editor.undo(),
            "y" if modifiers.control => self.editor.redo(),
            "c" | "x" if modifiers.control => {
                let selection = self.editor.selection();
                let text = self.editor.text_in(selection.clone());
                cx.write_to_clipboard(ClipboardItem::new_string(text.into()));
                if key == "x" && !selection.is_empty() {
                    self.editor.delete(false);
                }
            }
            "v" if modifiers.control => {
                if let Some(item) = cx.read_from_clipboard()
                    && let Some(text) = item.text()
                {
                    self.editor.insert(&text);
                    self.scroll_to_cursor();
                }
            }
            _ => return false,
        }
        // Any key press shows the caret again, as an IDE does.
        self.caret_visible.store(true, Ordering::Relaxed);
        if key == "backspace" || key == "delete" || key == "enter" {
            self.scroll_to_cursor();
        }
        cx.notify();
        true
    }

    /// Lines that fit in the visible area, for page up and page down.
    fn page(&self) -> usize {
        let height = f32::from(self.scroll.0.borrow().base_handle.bounds().size.height);
        (height / LINE_HEIGHT).floor().max(1.) as usize
    }

    fn scroll_to_cursor(&mut self) {
        let line = self.editor.buffer().line_of_offset(self.editor.cursor());
        self.scroll.scroll_to_item(line, gpui::ScrollStrategy::Center);
    }

    /// Start the caret blink, shown for one full period straight away.
    fn start_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_visible.store(true, Ordering::Relaxed);
        self.blink = Some(cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(BLINK).await;
                let Ok(keep) = view.update(cx, |view, cx| {
                    let next = !view.caret_visible.load(Ordering::Relaxed);
                    view.caret_visible.store(next, Ordering::Relaxed);
                    cx.notify();
                    true
                }) else {
                    return;
                };
                if !keep {
                    return;
                }
            }
        }));
    }

    /// The text offset a window point falls on, from the last paint.
    fn offset_at(&self, position: Point<Pixels>) -> Option<usize> {
        let row = self
            .rows
            .iter()
            .min_by_key(|row| {
                let distance = f32::from(position.y - row.top).abs();
                (distance * 1000.) as i64
            })?;
        let start = self.editor.buffer().line_start(row.line);
        let inside = row.shape.closest_index_for_x(position.x - row.text_left);
        Some(start + inside)
    }

    /// Mouse press inside the pane: place the caret, then drag to extend.
    fn pointer_down(&mut self, position: Point<Pixels>, extend: bool) {
        if let Some(offset) = self.offset_at(position) {
            self.editor.set_cursor(offset, extend);
            self.dragging = true;
        }
    }
}

impl Render for CodeEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus.is_focused(window) {
            if self.blink.is_none() {
                self.start_blink(cx);
            }
            self.caret_visible.store(true, Ordering::Relaxed);
        } else {
            self.blink = None;
        }
        self.rows.clear();

        let language = self.language;
        let path_display = self.path.display().to_string();
        let (line, column) = self.editor.line_column();
        let dirty = self.editor.is_dirty();
        let line_count = self.editor.buffer().line_count();
        let eol = self.editor.line_ending().as_str().to_owned();
        let modified = language.text("Modified");
        let undone = language.text("Undo");
        let redone = language.text("Redo");
        self.generation += 1;
        let generation = self.generation;

        let body = uniform_list(
            ("editor", generation),
            line_count,
            cx.processor(|view, range: Range<usize>, window, cx| {
                // The preview is recomputed here so it is ready before any
                // row shapes its text, and cached on the view.
                let code = view.highlighted_preview(cx);
                let cursor_line = view.editor.buffer().line_of_offset(view.editor.cursor());
                let revision = view.editor.revision();
                range
                    .map(|index| {
                        div()
                            .flex()
                            .flex_shrink_0()
                            .w_full()
                            .h(px(LINE_HEIGHT))
                            .when(cursor_line == index, |row| row.bg(color(SELECTED)))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(px(GUTTER_WIDTH))
                                    .pr_3()
                                    .text_right()
                                    .text_color(color(CODE_GUTTER))
                                    .child((index + 1).to_string()),
                            )
                            .child(EditorRow {
                                view: cx.entity(),
                                line: index,
                                revision,
                                code: code.clone(),
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    |view, event: &MouseDownEvent, window, cx| {
                                        view.focus.focus(window);
                                        view.pointer_down(event.position, event.modifiers.shift);
                                        cx.notify();
                                        cx.stop_propagation();
                                    },
                                ),
                            )
                            .on_mouse_move(cx.listener(
                                |view, event: &MouseMoveEvent, _, cx| {
                                    if view.dragging {
                                        if let Some(offset) = view.offset_at(event.position) {
                                            view.editor.set_cursor(offset, true);
                                            cx.notify();
                                        }
                                    }
                                },
                            ))
                            .on_mouse_up(MouseButton::Left, cx.listener(
                                |view, _, _, _| view.dragging = false,
                            ))
                            .on_mouse_up_out(MouseButton::Left, cx.listener(
                                |view, _, _, _| view.dragging = false,
                            ))
                    })
                    .collect()
            }),
        )
        .with_width_from_item(Some(0))
        .with_horizontal_sizing_behavior(gpui::ListHorizontalSizingBehavior::Unconstrained)
        .track_scroll(self.scroll.clone())
        .flex_1()
        .min_h_0()
        .font_family(code_font(cx))
        .text_size(px(12.))
        .line_height(px(LINE_HEIGHT))
        .whitespace_nowrap();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|view, event, _, cx| {
                view.key(event, cx);
            }))
            .child(body)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(color(BORDER))
                    .text_size(px(11.))
                    .text_color(color(MUTED))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(path_display),
                    )
                    .child(format!("Ln {line}, Col {column}"))
                    .child(format!("{line_count} lines"))
                    .child(eol)
                    .when(dirty, |status| {
                        status.child(div().text_color(color(ACCENT)).child(modified))
                    })
                    .when(view_can_undo(self), |status| {
                        status
                            .child(div().text_color(color(MUTED)).child(format!("{undone} · {redone}")))
                    }),
            )
    }
}

/// Whether an undo step exists; keeps the status bar hint honest.
fn view_can_undo(view: &CodeEditor) -> bool {
    view.can_undo() || view.can_redo()
}

/// One visible line of text. Shapes its own content so the caret, the selection
/// and the input handler all share one geometry.
struct EditorRow {
    view: Entity<CodeEditor>,
    line: usize,
    /// Revision the caller prepared the coloring for.
    #[allow(dead_code)]
    revision: u64,
    code: Option<Arc<CodePreview>>,
}

struct RowPaint {
    shape: ShapedLine,
    selection: Vec<Bounds<Pixels>>,
    caret: Option<Bounds<Pixels>>,
    text_left: Pixels,
    top: Pixels,
}

impl IntoElement for EditorRow {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for EditorRow {
    type RequestLayoutState = ();
    type PrepaintState = RowPaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(LINE_HEIGHT).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> RowPaint {
        let view = self.view.read(cx);
        let text = view.editor.buffer().line(self.line).to_owned();
        let style = window.text_style();
        let runs = highlight_runs(self.code.as_deref(), self.line, &text, &style.font());
        let shape = window.text_system().shape_line(
            text.clone().into(),
            style.font_size.to_pixels(window.rem_size()),
            &runs,
            None,
        );
        let start = view.editor.buffer().line_start(self.line);
        let selection = view.editor.selection();
        let mut rects = Vec::new();
        if !selection.is_empty() {
            let end_of_line = start + text.len();
            let from = selection.start.clamp(start, end_of_line);
            let to = selection.end.clamp(start, end_of_line);
            if to > from {
                let x0 = shape.x_for_index(from - start);
                let x1 = shape.x_for_index(to - start);
                rects.push(Bounds::from_corners(
                    point(bounds.left() + x0, bounds.top()),
                    point(bounds.left() + x1, bounds.bottom()),
                ));
            }
        }
        let cursor = view.editor.cursor();
        let caret = (cursor >= start && cursor <= start + text.len())
            .then(|| {
                let x = shape.x_for_index(cursor - start);
                Bounds::new(
                    point(bounds.left() + x, bounds.top()),
                    size(px(1.), bounds.size.height),
                )
            });
        RowPaint {
            shape,
            selection: rects,
            caret,
            text_left: bounds.left(),
            top: bounds.top(),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut RowPaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = self.view.read(cx);
        let focus = view.focus.clone();
        // The row is the input surface: clicks and IME land here.
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.view.clone()), cx);
        for rect in &prepaint.selection {
            window.paint_quad(fill(*rect, color(SELECTED)));
        }
        if let Some(caret) = prepaint.caret
            && focus.is_focused(window)
            && view.caret_visible.load(Ordering::Relaxed)
        {
            window.paint_quad(fill(caret, color(ACCENT)));
        }
        let _ = prepaint.shape.paint(bounds.origin, window.line_height(), window, cx);
        let shape = prepaint.shape.clone();
        let row = PaintedRow {
            line: self.line,
            top: prepaint.top,
            text_left: prepaint.text_left,
            shape,
        };
        self.view.update(cx, |view, _| view.rows.push(row));
    }
}

/// Build the color runs of one line from the preview's highlighting, filling the
/// gaps with the default code color.
fn highlight_runs(code: Option<&CodePreview>, line: usize, text: &str, font: &Font) -> Vec<TextRun> {
    let empty = CodeLine {
        text: SharedString::default(),
        number: SharedString::default(),
        highlights: Vec::new(),
    };
    let prepared = code.and_then(|code| code.lines.get(line)).unwrap_or(&empty);
    let mut runs = Vec::new();
    let mut offset = 0;
    for (range, style) in &prepared.highlights {
        if range.start > offset {
            runs.push(plain_run(offset..range.start, font, None));
        }
        runs.push(plain_run(range.start..range.end, font, style.color));
        offset = range.end;
    }
    if offset < text.len() {
        runs.push(plain_run(offset..text.len(), font, None));
    }
    if runs.is_empty() {
        runs.push(plain_run(0..text.len(), font, None));
    }
    runs
}

fn plain_run(range: Range<usize>, font: &Font, color: Option<Hsla>) -> TextRun {
    TextRun {
        len: range.end - range.start,
        font: font.clone(),
        color: color.unwrap_or_else(|| white()),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// The monospace family used by the code preview, resolved once per process.
fn code_font(cx: &App) -> SharedString {
    static FONT: std::sync::OnceLock<SharedString> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let available = cx.text_system().all_font_names();
        [
            "Cascadia Code",
            "JetBrains Mono",
            "Fira Code",
            "DejaVu Sans Mono",
            "Menlo",
            "Consolas",
            "monospace",
        ]
        .into_iter()
        .find(|family| {
            available
                .iter()
                .any(|name| name.to_lowercase().contains(&family.to_lowercase()))
        })
        .unwrap_or("monospace")
        .into()
    })
    .clone()
}

/// Byte offset to UTF-16 offset, the unit the platform input handler uses.
fn to_utf16(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].encode_utf16().count()
}

/// UTF-16 offset back to a byte offset, clamped to a character boundary.
fn from_utf16(text: &str, offset: usize) -> usize {
    let mut seen = 0;
    for (index, ch) in text.char_indices() {
        if seen >= offset {
            return index;
        }
        seen += ch.len_utf16();
    }
    text.len()
}

/// Typed characters and IME composition arrive here, not through the key
/// listener: the platform turns them into range replacements.
impl EntityInputHandler for CodeEditor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.editor.text();
        let start = from_utf16(&text, range.start);
        let end = from_utf16(&text, range.end).max(start);
        *actual = Some(to_utf16(&text, start)..to_utf16(&text, end));
        Some(text[start..end].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = self.editor.text();
        let selection = self.editor.selection();
        Some(UTF16Selection {
            range: to_utf16(&text, selection.start)..to_utf16(&text, selection.end),
            reversed: self.editor.anchor() > self.editor.cursor(),
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let marked = self.marked.as_ref()?;
        let text = self.editor.text();
        Some(to_utf16(&text, marked.start)..to_utf16(&text, marked.end))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let document = self.editor.text();
        let range = range
            .map(|range| from_utf16(&document, range.start)..from_utf16(&document, range.end))
            .unwrap_or_else(|| self.editor.selection());
        self.editor.replace_range(range, text);
        self.marked = None;
        self.caret_visible.store(true, Ordering::Relaxed);
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
            &mut self,
            range: Option<Range<usize>>,
            text: &str,
            selected: Option<Range<usize>>,
                            _: &mut Window,
                            cx: &mut Context<Self>,
                        ) {
                            // IME preedit: insert the composed text, then place the selection the
            // IME asks for inside it. The range is relative to the inserted text.
            let document = self.editor.text();
            let target = range
                .map(|range| from_utf16(&document, range.start)..from_utf16(&document, range.end))
                .unwrap_or_else(|| self.editor.selection());
            self.editor.replace_range(target, text);
            let caret = self.editor.cursor();
            if let Some(selected) = selected {
                let from = caret - text.len() + from_utf16(text, selected.start);
                let to = caret - text.len() + from_utf16(text, selected.end);
                self.editor.set_cursor(from, false);
                self.editor.set_cursor(to, true);
            }
            let _ = start;
            self.marked = Some(caret - text.len()..caret);
            self.caret_visible.store(true, Ordering::Relaxed);
            cx.notify();
        }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let document = self.editor.text();
        let start = from_utf16(&document, range.start);
        let end = from_utf16(&document, range.end);
        let line = self.editor.buffer().line_of_offset(start);
        let row = self.rows.iter().find(|row| row.line == line)?;
        let line_start = self.editor.buffer().line_start(line);
        let x0 = row.shape.x_for_index(start - line_start);
        let x1 = row.shape.x_for_index(end - line_start);
        Some(Bounds::from_corners(
            point(row.text_left + x0, row.top),
            point(row.text_left + x1, row.top + px(LINE_HEIGHT)),
        ))
    }
    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.offset_at(position).map(|offset| to_utf16(&self.editor.text(), offset))
    }
}

#[cfg(test)]
#[path = "../../../tests/ui/editor.rs"]
mod tests;