use super::components::{self, titlebar};
use crate::{app::FileManager, ui::theme::*};
use gpui::{Context, Decorations, Render, Window, div, prelude::*, px};

impl Render for FileManager {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.search_return_focus {
            self.search_return_focus = false;
            self.focus.focus(window);
        }
        self.sync_preview(cx);
        // Keep the current width when reversing a transition mid-animation.
        let now = std::time::Instant::now();
        if let Some((start, from, to)) = self.sidebar_transition {
            let progress = (now.duration_since(start).as_secs_f32() / 0.15).min(1.);
            let eased = 1. - (1. - progress).powi(3);
            self.sidebar_width = from + (to - from) * eased;
            if progress >= 1. {
                self.sidebar_transition = None;
            }
        }
        let preview_visible =
            self.details_open && self.location != crate::domain::location::Location::Workspaces;
        let target_width = if preview_visible { 0. } else { SIDEBAR_WIDTH };
        let current_target = self
            .sidebar_transition
            .map_or(self.sidebar_width, |(_, _, to)| to);
        if current_target != target_width {
            self.sidebar_transition = Some((now, self.sidebar_width, target_width));
        }
        if self.sidebar_transition.is_some() {
            window.request_animation_frame();
        }
        if !cx.has_active_drag() {
            self.external_drop = None;
            self.drop_hover = None;
        }
        window.set_window_title(&format!(
            "{} - Virial",
            self.location.description(self.language)
        ));
        let fullscreen = window.is_fullscreen();
        let tiled = match window.window_decorations() {
            Decorations::Client { tiling } => {
                tiling.top || tiling.bottom || tiling.left || tiling.right
            }
            Decorations::Server => false,
        };
        // The UI draws no invisible shadow margins, including in full screen.
        window.set_client_inset(px(0.));
        div()
            .id("file-manager")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_move(cx.listener(Self::move_window))
            .on_mouse_move(cx.listener(Self::resize_preview))
            .on_mouse_move(cx.listener(Self::update_marquee))
            .on_drag_move(cx.listener(Self::track_external_drag))
            .on_drag_move(cx.listener(
                |view, _: &gpui::DragMoveEvent<crate::state::mouse::FileDrag>, _, _| {
                    view.drop_hover = None;
                },
            ))
            .capture_any_mouse_up(cx.listener(Self::finish_mouse_selection))
            .capture_any_mouse_up(cx.listener(|view, _, _, _| {
                view.preview_resize = None;
            }))
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(Self::finish_mouse_selection),
            )
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(translucent(BACKGROUND, 0.97))
            .when(!fullscreen && !window.is_maximized() && !tiled, |root| {
                root.rounded_lg().border_1().border_color(color(BORDER))
            })
            .text_color(color(TEXT))
            .text_size(px(11.))
            .font_family("sans-serif")
            .when(!fullscreen, |root| root.child(self.titlebar(window, cx)))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(self.sidebar_width > 0., |layout| {
                        layout.child(
                            div()
                                .w(px(self.sidebar_width))
                                .h_full()
                                .flex_shrink_0()
                                .overflow_hidden()
                                .child(self.sidebar(cx)),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(self.toolbar(window, cx))
                            .child(components::folder_transition(
                                div()
                                    .flex()
                                    .flex_1()
                                    .min_h_0()
                                    .when(
                                        self.location
                                            == crate::domain::location::Location::Workspaces,
                                        |layout| layout.child(self.workspace_view(cx)),
                                    )
                                    .when(
                                        self.location
                                            != crate::domain::location::Location::Workspaces,
                                        |layout| layout.child(self.file_list(cx)),
                                    )
                                    .when(
                                        self.details_open
                                            && !self.preview_expanded
                                            && self.location
                                                != crate::domain::location::Location::Workspaces,
                                        |layout| {
                                            layout.child(components::reveal(
                                                self.preview_panel(false, window, cx),
                                                "preview-panel",
                                            ))
                                        },
                                    ),
                                self.navigation_generation,
                                self.loading,
                            )),
                    ),
            )
            .when(self.preview_expanded && self.details_open, |root| {
                root.child(components::reveal(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        // Keep preview controls clear of the window controls in the title bar.
                        .top(if fullscreen {
                            px(0.)
                        } else {
                            px(titlebar::HEIGHT)
                        })
                        .bg(color(BACKGROUND))
                        .flex()
                        .child(self.preview_panel(true, window, cx)),
                    "expanded-preview",
                ))
            })
            .children(titlebar::resize_handles(window))
            .children(self.context_overlay(window, cx))
            .children(self.global_search_overlay(window, cx))
    }
}
