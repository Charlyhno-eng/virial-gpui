//! Mouse selection and typed GPUI file drops.
use crate::{
    app::FileManager, domain::models::Entry, infrastructure::operations::Operation,
    state::selection::Selection, ui::theme::*,
};
use gpui::{
    Animation, AnimationExt, Context, Div, DragMoveEvent, ExternalPaths, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, Stateful, Window, div,
    point, prelude::*, px,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
pub(crate) struct FileDrag {
    pub(crate) paths: Arc<[PathBuf]>,
    pub(crate) label: String,
    pub(crate) count_label: String,
}
impl Render for FileDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(color(SURFACE))
            .border_1()
            .border_color(translucent(ACCENT_BLUE, 0.3))
            .text_size(px(11.))
            .text_color(color(TEXT))
            .child(self.label.clone())
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(color(MUTED))
                    .child(self.count_label.clone()),
            )
    }
}

pub(crate) struct ExternalDrop {
    paths: ExternalPaths,
    destination: Option<PathBuf>,
}

pub(crate) struct Marquee {
    pub(crate) start: Point<Pixels>, // List content coordinates, independent of scrolling.
    pub(crate) end: Point<Pixels>,
    baseline: Selection,
}

impl FileManager {
    pub(crate) fn selected_entries(&self) -> Vec<Entry> {
        self.selection
            .indices
            .iter()
            .filter_map(|index| self.entries.get(*index).cloned())
            .collect()
    }

    pub(crate) fn selected_paths(&self) -> Vec<PathBuf> {
        self.selection
            .indices
            .iter()
            .filter_map(|index| self.entries.get(*index))
            .map(|entry| entry.path.clone())
            .collect()
    }

    pub(crate) fn drag_payload(&self, index: usize, selected_paths: &Arc<[PathBuf]>) -> FileDrag {
        let paths = if self.selection.indices.contains(&index) {
            selected_paths.clone()
        } else {
            vec![self.entries[index].path.clone()].into()
        };
        let label = self.entries[index].name.clone();
        let count_label = self.language.selected_count(paths.len());
        FileDrag {
            paths,
            label,
            count_label,
        }
    }

    pub(crate) fn drop_target(
        &self,
        row: Stateful<Div>,
        directory: PathBuf,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let destination = directory.clone();
        let allowed = directory.clone();
        let external_destination = directory.clone();
        let external_hovered = self
            .external_drop
            .as_ref()
            .is_some_and(|drop| drop.destination.as_ref() == Some(&directory));
        let hovered = external_hovered || self.drop_hover.as_ref() == Some(&directory);
        let internal_destination = directory.clone();
        let view = cx.entity().downgrade();
        row.can_drop(move |value, _, cx| {
            view.upgrade().is_some_and(|view| {
                let view = view.read(cx);
                if view.busy || view.loading || view.dialog.is_some() {
                    return false;
                }
                if let Some(drag) = value.downcast_ref::<FileDrag>() {
                    !drag
                        .paths
                        .iter()
                        .any(|path| allowed == *path || allowed.starts_with(path))
                } else {
                    value.is::<ExternalPaths>()
                }
            })
        })
        .drag_over::<FileDrag>(|style, _, _, _| {
            style
                .bg(translucent(ACCENT_BLUE, 0.10))
                .border_color(translucent(ACCENT_BLUE, 0.35))
        })
        .when(hovered, |row| {
            row.relative()
                .child(
                    div()
                        .absolute()
                        .right_2()
                        .bottom_0()
                        .text_size(px(9.))
                        .text_color(color(ACCENT_BLUE))
                        .child(self.language.text("Drop here"))
                        .with_animation(
                            "drop-hint",
                            Animation::new(std::time::Duration::from_millis(150))
                                .with_easing(gpui::ease_out_quint()),
                            |hint, delta| hint.opacity(0.4 + 0.4 * delta).bottom(px(2. * delta)),
                        ),
                )
                .bg(translucent(ACCENT_BLUE, 0.10))
                .border_color(translucent(ACCENT_BLUE, 0.35))
        })
        .on_drag_move(
            cx.listener(move |view, event: &DragMoveEvent<FileDrag>, _, cx| {
                if event.bounds.contains(&event.event.position)
                    && !view.busy
                    && !view.loading
                    && view.dialog.is_none()
                    && !event.drag(cx).paths.iter().any(|path| {
                        internal_destination == *path || internal_destination.starts_with(path)
                    })
                {
                    view.drop_hover = Some(internal_destination.clone());
                    cx.notify();
                }
            }),
        )
        .on_drag_move(cx.listener(
            move |view, event: &DragMoveEvent<ExternalPaths>, window, cx| {
                let position = external_position(
                    event.event.position,
                    window.scale_factor(),
                    cx.compositor_name(),
                );
                if event.bounds.contains(&position)
                    && let Some(drop) = &mut view.external_drop
                {
                    drop.destination = Some(external_destination.clone());
                }
            },
        ))
        .on_drop(cx.listener(move |view, drag: &FileDrag, window, cx| {
            // Ctrl copies; ordinary internal drags move the selection.
            view.transfer_drop(
                drag.paths.to_vec(),
                destination.clone(),
                !window.modifiers().control,
                cx,
            );
        }))
    }

    pub(crate) fn track_external_drag(
        &mut self,
        event: &DragMoveEvent<ExternalPaths>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Drop targets run after the root in capture order. The deepest target
        // containing the corrected pointer wins, including sidebar and breadcrumbs.
        self.external_drop = Some(ExternalDrop {
            paths: event.drag(cx).clone(),
            destination: None,
        });
    }

    pub(crate) fn finish_mouse_selection(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        if let Some(drop) = self.external_drop.take()
            && cx.has_active_drag()
        {
            cx.stop_active_drag(window);
            cx.stop_propagation();
            if let Some(directory) = drop.destination {
                // External imports preserve the other application's originals.
                self.transfer_drop(drop.paths.paths().to_vec(), directory, false, cx);
            }
            cx.notify();
        }
        if self.marquee.take().is_some() {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn transfer_drop(
        &mut self,
        sources: Vec<PathBuf>,
        directory: PathBuf,
        cut: bool,
        cx: &mut Context<Self>,
    ) {
        if sources.is_empty() || self.busy || self.loading || self.dialog.is_some() {
            return;
        }
        self.menu = None;
        self.marquee = None;
        self.run_operation(
            Operation::Transfer {
                sources,
                directory,
                cut,
            },
            cx,
        );
    }

    pub(crate) fn begin_marquee(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview_focused = false;
        if self.busy || self.loading || self.dialog.is_some() {
            return;
        }
        let handle = self.scroll.0.borrow().base_handle.clone();
        let bounds = handle.bounds();
        let start = event.position - bounds.origin - handle.offset();
        let row_height = self.row_height();
        // Rows reserve a narrow left gutter for starting a selection rectangle.
        if event.position.x >= bounds.left() + px(14.)
            && start.y < px(row_height * self.entries.len() as f32)
        {
            return;
        }
        self.focus.focus(window);
        self.details_open = false;
        self.preview_expanded = false;
        self.menu = None;
        let baseline = if event.modifiers.control || event.modifiers.shift {
            self.selection.clone()
        } else {
            Selection::default()
        };
        self.selection = baseline.clone();
        self.marquee = Some(Marquee {
            start,
            end: start,
            baseline,
        });
        cx.notify();
    }

    pub(crate) fn update_marquee(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.marquee.is_none() {
            return;
        }
        if !event.dragging() {
            self.marquee = None;
            cx.notify();
            return;
        }
        let handle = self.scroll.0.borrow().base_handle.clone();
        let bounds = handle.bounds();
        let mut offset = handle.offset();
        let row_height = self.row_height();
        if event.position.y < bounds.top() + px(12.) {
            offset.y += px(row_height);
        } else if event.position.y > bounds.bottom() - px(12.) {
            offset.y -= px(row_height);
        }
        handle.set_offset(offset);
        let position = point(
            event.position.x.clamp(bounds.left(), bounds.right()),
            event.position.y.clamp(bounds.top(), bounds.bottom()),
        );
        let marquee = self.marquee.as_mut().unwrap();
        marquee.end = position - bounds.origin - handle.offset();
        let top = f32::from(marquee.start.y.min(marquee.end.y));
        let bottom = f32::from(marquee.start.y.max(marquee.end.y));
        let range = rectangle_rows(top, bottom, self.entries.len(), row_height);
        self.selection.rectangle(&marquee.baseline, range);
        cx.notify();
    }
}

// GPUI 0.2.2's X11 file-drop events contain device pixels, while its regular
// mouse events and Wayland drops already use logical pixels.
fn external_position(position: Point<Pixels>, scale: f32, compositor: &str) -> Point<Pixels> {
    if compositor == "X11" {
        position / scale
    } else {
        position
    }
}

fn rectangle_rows(top: f32, bottom: f32, count: usize, row_height: f32) -> std::ops::Range<usize> {
    if bottom - top < 1. {
        return 0..0;
    }
    let start = (top.max(0.) / row_height).floor() as usize;
    let end = (bottom.max(0.) / row_height).ceil() as usize;
    start.min(count)..end.min(count)
}

#[cfg(test)]
#[path = "../../tests/state/mouse.rs"]
mod tests;
