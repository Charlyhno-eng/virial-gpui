//! Application-owned Linux title bar and resize handles.
use crate::{app::FileManager, ui::icons::icon, ui::theme::*};
use gpui::{
    App, Context, CursorStyle, Div, ImageSource, MouseButton, Render, ResizeEdge, Resource,
    Stateful, Window, div, img, prelude::*, px,
};

struct ControlHint(&'static str);
impl Render for ControlHint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(color(SURFACE))
            .border_1()
            .border_color(color(BORDER))
            .text_color(color(TEXT))
            .text_size(px(11.))
            .child(self.0)
    }
}
fn control(
    id: &'static str,
    symbol: &'static str,
    label: &'static str,
    close: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(30.))
        .h(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .hover(move |style| style.bg(color(if close { ERROR_BG } else { HOVER })))
        .active(move |style| style.bg(color(if close { ERROR_BG } else { SELECTED })))
        .tooltip(move |_, cx: &mut App| cx.new(|_| ControlHint(label)).into())
        .child(icon(symbol, 14., if close { ERROR } else { MUTED }))
}
impl FileManager {
    pub(crate) fn move_window(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        if !event.dragging() {
            self.titlebar_drag = None;
            return;
        }
        if let Some(start) = self.titlebar_drag {
            if (event.position.x - start.x).abs() + (event.position.y - start.y).abs() > px(3.) {
                self.titlebar_drag = None;
                window.start_window_move();
            }
        }
    }
    pub(crate) fn titlebar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let maximized = window.is_maximized();
        div()
            .h(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(translucent(SURFACE, 0.55))
            .border_b_1()
            .border_color(color(BORDER))
            .child(
                div()
                    .id("titlebar-drag")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        img(ImageSource::Resource(Resource::Embedded(
                            "virial-gpui-logo.png".into(),
                        )))
                        .size(px(20.)),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Virial"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(color(MUTED))
                            .text_ellipsis()
                            .min_w_0()
                            .child(format!(
                                "— {}",
                                self.location.title(&self.home, self.language)
                            )),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, event: &gpui::MouseDownEvent, _, _| {
                            view.titlebar_drag = Some(event.position);
                        }),
                    )
                    .on_click(|event, window, _| {
                        if event.standard_click() && event.click_count() == 2 {
                            window.zoom_window();
                        }
                    })
                    .on_mouse_down(MouseButton::Right, |event, window, _| {
                        window.show_window_menu(event.position)
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .child(
                        control(
                            "window-fullscreen",
                            "fullscreen",
                            self.language.text("Full screen · F11"),
                            false,
                        )
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.titlebar_drag = None;
                            window.toggle_fullscreen();
                            cx.notify();
                        })),
                    )
                    .child(
                        control(
                            "window-minimize",
                            "minimize",
                            self.language.text("Minimize"),
                            false,
                        )
                        .on_click(|_, window, _| window.minimize_window()),
                    )
                    .child(
                        control(
                            "window-maximize",
                            if maximized { "restore" } else { "maximize" },
                            self.language
                                .text(if maximized { "Restore" } else { "Maximize" }),
                            false,
                        )
                        .on_click(|_, window, _| window.zoom_window()),
                    )
                    .child(
                        control("window-close", "close", self.language.text("Close"), true)
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
    }
}

pub(crate) fn resize_handles(window: &Window) -> Vec<Stateful<Div>> {
    if window.is_maximized() || window.is_fullscreen() {
        return Vec::new();
    }
    [
        ResizeEdge::Top,
        ResizeEdge::Bottom,
        ResizeEdge::Left,
        ResizeEdge::Right,
        ResizeEdge::TopLeft,
        ResizeEdge::TopRight,
        ResizeEdge::BottomLeft,
        ResizeEdge::BottomRight,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, edge)| {
        let handle = div().id(("window-resize", index)).absolute().occlude();
        let handle = match edge {
            ResizeEdge::Top => handle
                .top_0()
                .left(px(10.))
                .right(px(10.))
                .h(px(4.))
                .cursor(CursorStyle::ResizeUpDown),
            ResizeEdge::Bottom => handle
                .bottom_0()
                .left(px(10.))
                .right(px(10.))
                .h(px(4.))
                .cursor(CursorStyle::ResizeUpDown),
            ResizeEdge::Left => handle
                .left_0()
                .top(px(10.))
                .bottom(px(10.))
                .w(px(4.))
                .cursor(CursorStyle::ResizeLeftRight),
            ResizeEdge::Right => handle
                .right_0()
                .top(px(10.))
                .bottom(px(10.))
                .w(px(4.))
                .cursor(CursorStyle::ResizeLeftRight),
            ResizeEdge::TopLeft => handle
                .top_0()
                .left_0()
                .size(px(10.))
                .cursor(CursorStyle::ResizeUpLeftDownRight),
            ResizeEdge::TopRight => handle
                .top_0()
                .right_0()
                .size(px(10.))
                .cursor(CursorStyle::ResizeUpRightDownLeft),
            ResizeEdge::BottomLeft => handle
                .bottom_0()
                .left_0()
                .size(px(10.))
                .cursor(CursorStyle::ResizeUpRightDownLeft),
            ResizeEdge::BottomRight => handle
                .bottom_0()
                .right_0()
                .size(px(10.))
                .cursor(CursorStyle::ResizeUpLeftDownRight),
        };
        handle.on_mouse_down(MouseButton::Left, move |_, window, _| {
            window.start_window_resize(edge)
        })
    })
    .collect()
}
