//! Bottom status bar with the remote indicator, in the spirit of modern IDEs:
//! a plain glyph when offline, a colored `SSH: host` block when connected,
//! clicking opens the remote menu (connect / disconnect).

use crate::{
    app::FileManager,
    ui::{icons::icon, theme::*},
};
use gpui::{div, prelude::*, px, Context, Div};

/// Height of the status bar; title-bar-like typography keeps it discreet.
pub(crate) const STATUS_BAR_HEIGHT: f32 = 24.;

impl FileManager {
    pub(crate) fn status_bar(&self, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(STATUS_BAR_HEIGHT))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .border_t_1()
            .border_color(color(BORDER))
            .bg(translucent(SURFACE, 0.6))
            .text_size(px(10.))
            // Remote indicator, bottom-left corner.
            .child(self.remote_indicator(cx))
            // Right side placeholder: future context (item count, free space).
            .child(div())
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
}
