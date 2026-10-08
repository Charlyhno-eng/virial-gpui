use crate::platform::devices::{Action, Volume};
use crate::ui::components::button::navigation_button;
use crate::ui::components::section_label;
use crate::{app::FileManager, domain::location::Location, ui::icons::icon, ui::theme::*};
use gpui::{Context, Div, div, prelude::*, px};
use std::path::PathBuf;

struct DeviceTooltip(String);

/// Height of the sidebar footer row ("LOCAL FILES" line); the remote menu
/// anchors right above it.
pub(crate) const SIDEBAR_FOOTER_HEIGHT: f32 = 28.;

impl gpui::Render for DeviceTooltip {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .max_w(px(360.))
            .rounded_sm()
            .bg(color(SURFACE))
            .border_1()
            .border_color(color(BORDER))
            .text_size(px(11.))
            .text_color(color(TEXT))
            .child(self.0.clone())
    }
}

impl FileManager {
    pub(crate) fn sidebar(&self, cx: &mut Context<Self>) -> Div {
        let available_space = crate::infrastructure::storage::available_space(&self.home).ok();
        div()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(translucent(SIDEBAR, 0.45))
            .border_r_1()
            .border_color(color(BORDER))
            .child(
                div()
                    .id("places")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .child(section_label(self.language.text("PLACES")))
                    .children(
                        self.places
                            .iter()
                            .take(self.places.len() - 1)
                            .enumerate()
                            .map(|(index, place)| {
                                self.place(
                                    index,
                                    place.label,
                                    place.icon,
                                    place.path.clone().into(),
                                    cx,
                                )
                            }),
                    )
                    .child(self.place(101, "Recent", "recent", Location::Recent, cx))
                    .child(self.place(102, "Trash", "trash", Location::Trash, cx))
                    .child(section_label(self.language.text("DEVICES")))
                    .child(self.place(100, "File System", "drive", PathBuf::from("/").into(), cx))
                    .children(
                        self.devices
                            .iter()
                            .map(|device| self.device_place(device, cx)),
                    )
                    .when_some(self.device_error.as_ref(), |sidebar, error| {
                        sidebar.child(
                            div()
                                .px_3()
                                .py_2()
                                .text_size(px(10.))
                                .text_color(color(ERROR))
                                .child(error.clone()),
                        )
                    })
                    .child(section_label(self.language.text("WORKSPACES")))
                    .child(self.place(103, "Workspaces", "view", Location::Workspaces, cx)),
            )
            .child(
                div()
                    .px_3()
                    .h(px(SIDEBAR_FOOTER_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(color(BORDER))
                    .text_size(px(9.5))
                    .text_color(color(MUTED))
                    // Remote indicator leads the row, split from the location
                    // label by a thin vertical rule: two zones, one line.
                    .child(self.remote_indicator(cx))
                    .child(div().w(px(1.)).h(px(14.)).bg(color(BORDER)))
                    .child(self.language.text("LOCAL FILES"))
                    .when_some(available_space, |footer, bytes| {
                        footer.child(div().flex_1().min_w_0().text_ellipsis().text_right().child(
                            format!(
                                "{} {}",
                                self.language.size(Some(bytes)),
                                self.language.text("free")
                            ),
                        ))
                    }),
            )
    }

    /// The `><` badge: muted when idle, amber while connecting, accent block
    /// once connected — click opens the remote menu.
    fn remote_indicator(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let ssh = &self.ssh;
        let (label, symbol_color, badge, connected_host) = match &ssh.activity {
            crate::state::ssh::SshActivity::Idle => (None, MUTED, false, None),
            crate::state::ssh::SshActivity::Connecting(host) => (
                Some(self.language.text("Connecting…").to_string()),
                ACCENT,
                false,
                Some(host.clone()),
            ),
            crate::state::ssh::SshActivity::Connected(id) => (
                Some(format!("SSH: {id}")),
                ACCENT,
                true,
                Some(id.to_string()),
            ),
        };
        div()
            .id("remote-indicator")
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .h(px(18.))
            .rounded_md()
            .text_size(px(10.))
            .text_color(color(if badge { BACKGROUND } else { TEXT }))
            .when(badge, |badge| badge.bg(color(ACCENT)))
            .cursor_pointer()
            .hover(|style| style.bg(color(if badge { ACCENT } else { HOVER })))
            .tooltip(move |_, cx| {
                let hint = connected_host.clone();
                cx.new(move |_| {
                    let text = hint.unwrap_or_else(|| "Remote connection".into());
                    super::modal::StatusTooltip(text)
                })
                .into()
            })
            .child(icon("remote", 12., if badge { BACKGROUND } else { symbol_color }))
            .when_some(label, |badge, label| badge.child(label))
            .on_click(cx.listener(|view, _, window, cx| view.open_remote_menu(window, cx)))
    }

    fn place(
        &self,
        index: usize,
        label: &'static str,
        symbol: &'static str,
        location: Location,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let active = if let Some(path) = location.directory() {
            self.location.directory().is_some_and(|current| {
                self.active_sidebar_path(current)
                    .is_some_and(|active| active == path)
            })
        } else {
            self.location == location
        };
        let destination = location.directory().map(|path| path.to_path_buf());
        div()
            .id(("place", index))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(28.))
            .mb(px(2.))
            .rounded_md()
            .cursor_pointer()
            .text_size(px(10.5))
            .text_color(color(if active { ACCENT } else { TEXT }))
            .when(active, |row| row.bg(color(SELECTED)))
            .hover(|style| style.bg(color(if active { SELECTED } else { HOVER })))
            .active(|style| style.bg(color(SELECTED)))
            .child(icon(symbol, 16., if active { ACCENT } else { MUTED }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(self.language.text(label)),
            )
            .when(active, |row| {
                row.child(div().w(px(3.)).h(px(12.)).rounded_full().bg(color(ACCENT)))
            })
            .when_some(destination, |row, directory| {
                self.drop_target(row, directory, cx)
            })
            .on_click(cx.listener(move |view, _, window, cx| {
                view.focus.focus(window);
                view.navigate_location(location.clone(), cx);
            }))
    }

    fn active_sidebar_path<'a>(&'a self, current: &std::path::Path) -> Option<&'a std::path::Path> {
        self.places
            .iter()
            .map(|place| place.path.as_path())
            .chain(
                self.devices
                    .iter()
                    .flat_map(|device| device.mountpoints.iter().map(|mount| mount.as_path())),
            )
            .filter(|path| current.starts_with(path))
            .max_by_key(|path| path.components().count())
    }

    fn device_place(&self, device: &Volume, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let active = self
            .location
            .directory()
            .and_then(|directory| self.active_sidebar_path(directory))
            .is_some_and(|path| device.mountpoints.iter().any(|mount| mount == path));
        let mounted = !device.mountpoints.is_empty();
        let enabled = !self.busy && self.dialog.is_none();
        let open = device.clone();
        let unmount = device.clone();
        let remove = device.clone();
        let hint = format!("{} · {}", device.label, device.device.display());
        div()
            .id(gpui::SharedString::from(format!(
                "device-{}",
                device.object
            )))
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py(px(2.))
            .mb(px(2.))
            .rounded_md()
            .text_size(px(10.5))
            .text_color(color(if active { ACCENT } else { TEXT }))
            .when(active, |row| row.bg(color(SELECTED)))
            .child(
                div()
                    .id(gpui::SharedString::from(format!("open-{}", device.object)))
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_1()
                    .py(px(2.))
                    .rounded_sm()
                    .when(enabled, |row| {
                        row.cursor_pointer().hover(|style| style.bg(color(HOVER)))
                    })
                    .when(!enabled, |row| row.opacity(0.5))
                    .tooltip(move |_, cx| cx.new(|_| DeviceTooltip(hint.clone())).into())
                    .child(icon("drive", 16., if active { ACCENT } else { MUTED }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_ellipsis().child(device.label.clone()))
                            .child(
                                div()
                                    .text_ellipsis()
                                    .text_size(px(8.5))
                                    .text_color(color(MUTED))
                                    .child(format!(
                                        "{} · {}",
                                        self.language.size(Some(device.size)),
                                        self.language.text(if mounted {
                                            "Mounted"
                                        } else {
                                            "Click to mount"
                                        })
                                    )),
                            ),
                    )
                    .when_some(device.mountpoints.first().cloned(), |row, directory| {
                        self.drop_target(row, directory, cx)
                    })
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.focus.focus(window);
                        view.device_action(open.clone(), Action::Open, cx);
                    })),
            )
            .when(mounted, |row| {
                row.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "unmount-{}",
                            device.object
                        )))
                        .child(
                            navigation_button(
                                "unmount",
                                "close",
                                self.language.text("Unmount volume"),
                                enabled,
                            )
                            .px_1()
                            .w(px(20.))
                            .h(px(24.))
                            .on_click(cx.listener(
                                move |view, _, _, cx| {
                                    cx.stop_propagation();
                                    view.device_action(unmount.clone(), Action::Unmount, cx);
                                },
                            )),
                        ),
                )
            })
            .when(device.can_power_off, |row| {
                row.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "remove-{}",
                            device.object
                        )))
                        .child(
                            navigation_button(
                                "safely-remove",
                                "eject",
                                self.language.text("Safely remove drive (all volumes)"),
                                enabled,
                            )
                            .px_1()
                            .w(px(20.))
                            .h(px(24.))
                            .on_click(cx.listener(
                                move |view, _, _, cx| {
                                    cx.stop_propagation();
                                    view.device_action(remove.clone(), Action::SafelyRemove, cx);
                                },
                            )),
                        ),
                )
            })
    }
}
