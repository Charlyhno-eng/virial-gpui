//! A window owns tabs; each tab retains two independent file-manager entities.
use crate::{
    app::FileManager,
    state::navigation::{Tabs, ViewMode},
    ui::{components::titlebar, theme::*},
};
use gpui::{
    AppContext, Context, Entity, KeyDownEvent, MouseButton, Render, Window, div, prelude::*, px,
};
use std::path::PathBuf;

struct Tab {
    left: Entity<FileManager>,
    right: Option<Entity<FileManager>>,
    split: bool,
    right_active: bool,
    ratio: f32,
}
impl Tab {
    fn pane(&self) -> Entity<FileManager> {
        if self.split && self.right_active {
            self.right.as_ref().unwrap().clone()
        } else {
            self.left.clone()
        }
    }
    fn panes(&self) -> impl Iterator<Item = &Entity<FileManager>> {
        std::iter::once(&self.left).chain(self.right.iter())
    }
}
pub(crate) struct BrowserWindow {
    tabs: Tabs<Tab>,
    resizing: bool,
}
impl BrowserWindow {
    pub(crate) fn new(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let left = cx.new(|cx| FileManager::new(path, window, cx));
        Self::from_pane(left, window, cx)
    }

    fn from_pane(left: Entity<FileManager>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view = Self {
            tabs: Tabs::new(Tab {
                left,
                right: None,
                split: false,
                right_active: false,
                ratio: 0.5,
            }),
            resizing: false,
        };
        view.observe_pane(&view.tabs.items[0].left, window, cx);
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |view, cx| {
                for tab in 0..view.tabs.items.len() {
                    if !view.can_close(tab, window, cx) {
                        return false;
                    }
                }
                true
            })
            .unwrap_or(true)
        });
        view
    }
    fn observe_pane(&self, pane: &Entity<FileManager>, window: &Window, cx: &mut Context<Self>) {
        let mut was_running = false;
        cx.observe(pane, move |view, changed, cx| {
            let pane = changed.read(cx);
            let running = pane.busy || pane.queue_running;
            let completed = was_running && !running;
            was_running = running;
            if completed {
                let others = view
                    .tabs
                    .items
                    .iter()
                    .flat_map(Tab::panes)
                    .filter(|pane| *pane != &changed)
                    .cloned()
                    .collect::<Vec<_>>();
                for pane in others {
                    pane.update(cx, |pane, cx| {
                        if !pane.busy
                            && !pane.queue_running
                            && !pane.loading
                            && pane.editor.is_none()
                            && pane.dialog.is_none()
                        {
                            pane.refresh(cx);
                        }
                    });
                }
            }
            cx.notify();
        })
        .detach();
        cx.subscribe_in(
            pane,
            window,
            |view, _, _: &crate::state::app_state::CloseWindow, window, cx| {
                for tab in 0..view.tabs.items.len() {
                    if !view.can_close(tab, window, cx) {
                        return;
                    }
                }
                window.remove_window();
            },
        )
        .detach();
    }
    fn active(&self) -> Entity<FileManager> {
        self.tabs.items[self.tabs.active].pane()
    }
    fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.active().read(cx).focus.focus(window);
        cx.notify();
    }
    fn new_pane(&self, window: &mut Window, cx: &mut Context<Self>) -> Entity<FileManager> {
        let source = self.active();
        let source = source.read(cx);
        let location = source.location.clone();
        let path = source
            .location
            .directory()
            .map(|path| path.to_path_buf())
            .unwrap_or_else(|| source.home.clone());
        let clipboard = source.clipboard.clone();
        let home = source.home.clone();
        let data_home = source.data_home.clone();
        let store = source.ssh.store.clone();
        let runtime = source.ssh.runtime.clone();
        let activity = source.ssh.activity.clone();
        let mode = source.view_mode;
        let pane = cx.new(|cx| {
            let mut pane = FileManager::with_paths(path, home, data_home, false, window, cx);
            pane.clipboard = clipboard;
            pane.ssh.store = store;
            pane.ssh.runtime = runtime;
            pane.ssh.activity = activity;
            pane.view_mode = mode;
            pane.navigate_location(location, cx);
            pane
        });
        self.observe_pane(&pane, window, cx);
        pane
    }
    fn pause_tab(&self, cx: &mut Context<Self>) {
        for pane in self.tabs.items[self.tabs.active].panes() {
            pane.update(cx, |pane, _| pane.cancel_pending_preview());
        }
    }
    fn add_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let left = self.new_pane(window, cx);
        self.pause_tab(cx);
        self.tabs.push(Tab {
            left,
            right: None,
            split: false,
            right_active: false,
            ratio: 0.5,
        });
        self.focus(window, cx);
    }
    fn select_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.pause_tab(cx);
        self.tabs.select(index);
        self.focus(window, cx);
    }
    fn can_close(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let blocked = self.tabs.items[index].panes().find_map(|pane| {
            let view = pane.read(cx);
            if view.busy || view.queue_running {
                Some((
                    pane.clone(),
                    "Wait for the current operation to finish before closing",
                ))
            } else if view.editor_is_dirty(cx) {
                Some((pane.clone(), "Save the file before closing it"))
            } else {
                None
            }
        });
        if let Some((pane, message)) = blocked {
            self.tabs.select(index);
            let tab = &mut self.tabs.items[index];
            if tab.right.as_ref() == Some(&pane) {
                tab.split = true;
                tab.right_active = true;
            } else {
                tab.right_active = false;
            }
            pane.update(cx, |pane, cx| {
                pane.error = Some(pane.language.text(message).into());
                pane.focus.focus(window);
                cx.notify();
            });
            cx.notify();
            return false;
        }
        true
    }
    fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.items.len() > 1 && self.can_close(index, window, cx) {
            self.tabs.close(index);
            self.focus(window, cx);
        }
    }
    fn toggle_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.tabs.active;
        if self.tabs.items[index].right.is_none() {
            let pane = self.new_pane(window, cx);
            self.tabs.items[index].right = Some(pane);
        }
        let tab = &mut self.tabs.items[index];
        if tab.split {
            tab.right
                .as_ref()
                .unwrap()
                .update(cx, |pane, _| pane.cancel_pending_preview());
        }
        tab.split = !tab.split;
        tab.right_active = tab.split;
        self.focus(window, cx);
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let m = event.keystroke.modifiers;
        if !(m.control || m.platform) || m.alt {
            return;
        }
        match key {
            "t" if !m.shift => self.add_tab(window, cx),
            "w" if !m.shift => self.close_tab(self.tabs.active, window, cx),
            "tab" => {
                self.pause_tab(cx);
                self.tabs.cycle(m.shift);
                self.focus(window, cx);
            }
            "1" | "2" if !m.shift => {
                let mode = if key == "1" {
                    ViewMode::List
                } else {
                    ViewMode::Grid
                };
                self.active()
                    .update(cx, |pane, cx| pane.set_view_mode(mode, cx));
            }
            "4" if !m.shift => self.toggle_split(window, cx),
            "6" if !m.shift && self.tabs.items[self.tabs.active].split => {
                let tab = &mut self.tabs.items[self.tabs.active];
                tab.right_active = !tab.right_active;
                self.focus(window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}
impl Render for BrowserWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active();
        let language = active.read(cx).language;
        let modifier = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        };
        window.set_window_title(&format!(
            "{} - Virial",
            active
                .read(cx)
                .location
                .description(active.read(cx).language)
        ));
        let title = active.update(cx, |pane, cx| pane.titlebar(window, cx));
        let sidebar = active.update(cx, |pane, cx| pane.sidebar(cx));
        let index = self.tabs.active;
        let tab = &self.tabs.items[index];
        let split = tab.split;
        let content_width = (f32::from(window.viewport_size().width) - SIDEBAR_WIDTH - 5.).max(1.);
        let ratio = crate::state::navigation::split_ratio(tab.ratio, content_width);
        let right_active = tab.right_active;
        let left = tab.left.clone();
        let right = tab.right.clone();
        for (pane, width) in
            std::iter::once((left.clone(), content_width * if split { ratio } else { 1. })).chain(
                right
                    .iter()
                    .cloned()
                    .map(|pane| (pane, content_width * (1. - ratio))),
            )
        {
            pane.update(cx, |pane, cx| {
                let compact = width < 480.;
                if pane.compact != compact {
                    pane.compact = compact;
                    cx.notify();
                }
            });
        }
        let tab_bar = div()
            .id("tabs")
            .debug_selector(|| "tabs".into())
            .h(px(26.))
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap_1()
            .border_b_1()
            .border_color(color(BORDER))
            .child(
                div()
                    .id("tab-strip")
                    .h_full()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_x_scroll()
                    .children(self.tabs.items.iter().enumerate().map(|(i, tab)| {
                        let pane = tab.pane();
                        let pane = pane.read(cx);
                        div()
                            .id(("tab", i))
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .max_w(px(220.))
                            .gap_2()
                            .px_3()
                            .h_full()
                            .cursor_pointer()
                            .bg(color(if i == index { SELECTED } else { SURFACE }))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .child(pane.location.title(&pane.home, pane.language)),
                            )
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.select_tab(i, window, cx)
                            }))
                            .when(self.tabs.items.len() > 1, |el| {
                                el.child(
                                    div()
                                        .id(("close-tab", i))
                                        .px_1()
                                        .child("×")
                                        .tooltip(move |_, cx| {
                                            cx.new(|_| {
                                                crate::ui::components::modal::StatusTooltip(
                                                    format!(
                                                        "{} · {modifier}+W",
                                                        language.text("Close tab")
                                                    ),
                                                )
                                            })
                                            .into()
                                        })
                                        .hover(|s| s.bg(color(HOVER)))
                                        .on_click(cx.listener(move |view, _, window, cx| {
                                            cx.stop_propagation();
                                            view.close_tab(i, window, cx);
                                        })),
                                )
                            })
                    })),
            )
            .child(
                div()
                    .id("new-tab")
                    .h_full()
                    .w(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child("+")
                    .tooltip(move |_, cx| {
                        cx.new(|_| {
                            crate::ui::components::modal::StatusTooltip(format!(
                                "{} · {modifier}+T",
                                language.text("New tab")
                            ))
                        })
                        .into()
                    })
                    .on_click(cx.listener(|view, _, window, cx| view.add_tab(window, cx))),
            )
            .child(
                div()
                    .id("split-view")
                    .h_full()
                    .w(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .bg(color(if split { SELECTED } else { SURFACE }))
                    .child("◫")
                    .tooltip(move |_, cx| {
                        cx.new(|_| {
                            crate::ui::components::modal::StatusTooltip(format!(
                                "{} · {modifier}+4",
                                language.text("Two panes")
                            ))
                        })
                        .into()
                    })
                    .on_click(cx.listener(|view, _, window, cx| view.toggle_split(window, cx))),
            );
        div()
            .id("browser-window")
            .capture_key_down(cx.listener(Self::key_down))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(color(BACKGROUND))
            .text_color(color(TEXT))
            .text_size(px(11.))
            .font_family("sans-serif")
            .on_mouse_move(
                cx.listener(|view, event: &gpui::MouseMoveEvent, window, cx| {
                    view.active()
                        .update(cx, |pane, cx| pane.move_window(event, window, cx));
                    if view.resizing && event.dragging() {
                        let width = f32::from(window.viewport_size().width) - SIDEBAR_WIDTH - 5.;
                        view.tabs.items[view.tabs.active].ratio =
                            crate::state::navigation::split_ratio(
                                (f32::from(event.position.x) - SIDEBAR_WIDTH) / width.max(1.),
                                width,
                            );
                        cx.notify();
                    }
                }),
            )
            .capture_any_mouse_up(cx.listener(|view, _, _, _| view.resizing = false))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, _, _| view.resizing = false),
            )
            .when(!window.is_fullscreen(), |el| el.child(title))
            .child(tab_bar)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .w(px(SIDEBAR_WIDTH))
                            .h_full()
                            .flex_shrink_0()
                            .child(sidebar),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                div()
                                    .id("left-pane")
                                    .debug_selector(|| "left-pane".into())
                                    .flex()
                                    .flex_col()
                                    .h_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .when(split, |el| {
                                        el.w(px(content_width * ratio)).border_t_2().border_color(
                                            color(if !right_active { ACCENT_BLUE } else { BORDER }),
                                        )
                                    })
                                    .when(!split, |el| el.flex_1())
                                    .capture_any_mouse_down(cx.listener(|view, _, window, cx| {
                                        view.tabs.items[view.tabs.active].right_active = false;
                                        view.focus(window, cx);
                                    }))
                                    .child(left),
                            )
                            .when(split, |el| {
                                el.child(
                                    div()
                                        .id("pane-divider")
                                        .debug_selector(|| "pane-divider".into())
                                        .w(px(5.))
                                        .h_full()
                                        .flex_shrink_0()
                                        .cursor_col_resize()
                                        .bg(color(BORDER))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                |view, event: &gpui::MouseDownEvent, _, cx| {
                                                    if event.click_count == 2 {
                                                        view.tabs.items[view.tabs.active].ratio =
                                                            0.5;
                                                    } else {
                                                        view.resizing = true;
                                                    }
                                                    cx.notify();
                                                },
                                            ),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("right-pane")
                                        .debug_selector(|| "right-pane".into())
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .h_full()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .border_t_2()
                                        .border_color(color(if right_active {
                                            ACCENT_BLUE
                                        } else {
                                            BORDER
                                        }))
                                        .capture_any_mouse_down(cx.listener(
                                            |view, _, window, cx| {
                                                view.tabs.items[view.tabs.active].right_active =
                                                    true;
                                                view.focus(window, cx);
                                            },
                                        ))
                                        .child(right.unwrap()),
                                )
                            }),
                    ),
            )
            .children(titlebar::resize_handles(window))
            .children(active.update(cx, |pane, cx| pane.context_overlay(window, cx)))
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../tests/ui/browser_window.rs"]
mod tests;
