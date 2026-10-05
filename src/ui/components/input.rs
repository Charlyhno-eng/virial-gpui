//! Single-line filename editor with clipboard, Unicode selection and IME support.
use crate::ui::theme::*;
use gpui::{prelude::*, *};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub struct NameInput {
    pub text: String,
    pub(crate) placeholder: String,
    pub(crate) compact: bool,
    pub(crate) search_icon: bool,
    focus: FocusHandle,
    selection: Range<usize>,
    anchor: usize,
    marked: Option<Range<usize>>,
    line: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    dragging: bool,
}
impl NameInput {
    pub fn new(text: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = Self::new_unfocused(text, cx);
        input.focus.focus(window);
        input
    }
    pub(crate) fn for_rename(
        text: String,
        directory: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let end = rename_selection_end(&text, directory);
        let mut input = Self::new(text, window, cx);
        input.compact = true;
        input.selection = 0..end;
        input
    }
    pub fn new_unfocused(text: String, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let end = text.len();
        Self {
            text,
            placeholder: String::new(),
            compact: false,
            search_icon: false,
            focus,
            selection: 0..end,
            anchor: 0,
            marked: None,
            line: None,
            bounds: None,
            dragging: false,
        }
    }
    pub(crate) fn focus(&self, window: &mut Window) {
        self.focus.focus(window);
    }

    pub(crate) fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    pub(crate) fn clear(&mut self) {
        self.text.clear();
        self.move_to(0, false);
    }

    fn offset(&self, utf16: usize) -> usize {
        let mut count = 0;
        for (index, ch) in self.text.char_indices() {
            if count >= utf16 {
                return index;
            }
            count += ch.len_utf16();
        }
        self.text.len()
    }
    fn range(&self, utf16: Range<usize>) -> Range<usize> {
        self.offset(utf16.start)..self.offset(utf16.end)
    }
    fn utf16(&self, range: Range<usize>) -> Range<usize> {
        self.text[..range.start].encode_utf16().count()
            ..self.text[..range.end].encode_utf16().count()
    }
    fn cursor(&self) -> usize {
        if self.anchor == self.selection.end {
            self.selection.start
        } else {
            self.selection.end
        }
    }
    fn move_to(&mut self, index: usize, extend: bool) {
        if !extend {
            self.anchor = index;
        }
        self.selection = self.anchor.min(index)..self.anchor.max(index);
        self.marked = None;
    }
    fn replace(&mut self, range: Range<usize>, text: &str) {
        let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
        self.text.replace_range(range.clone(), &text);
        self.move_to(range.start + text.len(), false);
    }
    fn index(&self, position: Point<Pixels>) -> usize {
        self.bounds
            .zip(self.line.as_ref())
            .map(|(bounds, line)| line.closest_index_for_x(position.x - bounds.left()))
            .unwrap_or(0)
            .min(self.text.len())
    }
    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let cursor = self.cursor();
        let previous = self
            .text
            .grapheme_indices(true)
            .rev()
            .find(|(i, _)| *i < cursor)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let next = self
            .text
            .grapheme_indices(true)
            .find(|(i, _)| *i > cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        match key {
            "a" if modifiers.control => {
                self.anchor = 0;
                self.selection = 0..self.text.len();
            }
            "c" | "x" if modifiers.control => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    self.text[self.selection.clone()].into(),
                ));
                if key == "x" {
                    self.replace(self.selection.clone(), "");
                }
            }
            "v" if modifiers.control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace(self.selection.clone(), &text);
                }
            }
            "left" => self.move_to(
                if !modifiers.shift && !self.selection.is_empty() {
                    self.selection.start
                } else {
                    previous
                },
                modifiers.shift,
            ),
            "right" => self.move_to(
                if !modifiers.shift && !self.selection.is_empty() {
                    self.selection.end
                } else {
                    next
                },
                modifiers.shift,
            ),
            "home" => self.move_to(0, modifiers.shift),
            "end" => self.move_to(self.text.len(), modifiers.shift),
            "backspace" | "delete" => {
                let range = if !self.selection.is_empty() {
                    self.selection.clone()
                } else if key == "backspace" {
                    previous..cursor
                } else {
                    cursor..next
                };
                self.replace(range, "");
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}
impl EntityInputHandler for NameInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range(range);
        *actual = Some(self.utf16(range.clone()));
        Some(self.text[range].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.utf16(self.selection.clone()),
            reversed: self.anchor == self.selection.end,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|range| self.utf16(range))
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
        let range = range
            .map(|range| self.range(range))
            .or(self.marked.clone())
            .unwrap_or(self.selection.clone());
        self.replace(range, text);
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
        let range = range
            .map(|range| self.range(range))
            .or(self.marked.clone())
            .unwrap_or(self.selection.clone());
        let start = range.start;
        self.replace(range, text);
        let end = self.cursor();
        self.marked = (start < end).then_some(start..end);
        if let Some(selected) = selected {
            let local = &self.text[start..end];
            let offset = |value| {
                let mut count = 0;
                for (i, ch) in local.char_indices() {
                    if count >= value {
                        return i;
                    }
                    count += ch.len_utf16();
                }
                local.len()
            };
            self.selection = start + offset(selected.start)..start + offset(selected.end);
            self.anchor = self.selection.start;
        }
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range(range);
        let line = self.line.as_ref()?;
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }
    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.utf16(0..self.index(position)).end)
    }
}
struct InputElement(Entity<NameInput>);
impl IntoElement for InputElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for InputElement {
    type RequestLayoutState = ();
    type PrepaintState = ShapedLine;
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
        style.size.height = px(if self.0.read(cx).compact { 20. } else { 24. }).into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> ShapedLine {
        let input = self.0.read(cx);
        let style = window.text_style();
        let placeholder = input.text.is_empty();
        let text = if placeholder {
            &input.placeholder
        } else {
            &input.text
        };
        window.text_system().shape_line(
            text.clone().into(),
            style.font_size.to_pixels(window.rem_size()),
            &[TextRun {
                len: text.len(),
                font: style.font(),
                color: if placeholder {
                    color(MUTED)
                } else {
                    style.color
                },
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        line: &mut ShapedLine,
        window: &mut Window,
        cx: &mut App,
    ) {
        let input = self.0.read(cx);
        let selection = input.selection.clone();
        let cursor = input.cursor();
        let focus = input.focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.0.clone()), cx);
        if !selection.is_empty() {
            window.paint_quad(fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(selection.start),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(selection.end),
                        bounds.bottom(),
                    ),
                ),
                color(SELECTED),
            ));
        } else if focus.is_focused(window) {
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left() + line.x_for_index(cursor), bounds.top()),
                    size(px(1.), bounds.size.height),
                ),
                color(ACCENT),
            ));
        }
        let _ = line.paint(bounds.origin, window.line_height(), window, cx);
        self.0.update(cx, |input, _| {
            input.line = Some(line.clone());
            input.bounds = Some(bounds);
        });
    }
}
impl Render for NameInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("name-input")
            .track_focus(&self.focus)
            .w_full()
            .when(self.compact, |input| input.px_2().py(px(2.)))
            .when(!self.compact, |input| input.p_2())
            .bg(color(BACKGROUND))
            .border_1()
            .border_color(color(if self.compact { BORDER } else { ACCENT }))
            .rounded_md()
            .cursor(CursorStyle::IBeam)
            .overflow_hidden()
            .text_size(px(11.))
            .line_height(px(if self.compact { 20. } else { 24. }))
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|input, event: &MouseDownEvent, window, cx| {
                    input.focus.focus(window);
                    let index = input.index(event.position);
                    input.move_to(index, event.modifiers.shift);
                    input.dragging = true;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|input, event: &MouseMoveEvent, _, cx| {
                if input.dragging {
                    input.move_to(input.index(event.position), true);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|input, _, _, _| input.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|input, _, _, _| input.dragging = false),
            )
            .when(self.search_icon, |input| {
                input
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(crate::ui::icons::icon("search", 14., MUTED))
                    .child(div().flex_1().min_w_0().child(InputElement(cx.entity())))
            })
            .when(!self.search_icon, |input| input.child(InputElement(cx.entity())))
    }
}

// A leading dot alone is part of the name, not an extension separator.
fn rename_selection_end(name: &str, directory: bool) -> usize {
    if directory {
        name.len()
    } else {
        name.rfind('.')
            .filter(|index| *index > 0)
            .unwrap_or(name.len())
    }
}

#[cfg(test)]
#[path = "../../../tests/ui/input.rs"]
mod tests;
