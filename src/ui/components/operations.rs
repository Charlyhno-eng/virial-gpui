use crate::{
    app::FileManager,
    infrastructure::{
        operations::Operation,
        progress::{Phase, Resolution},
        queue,
    },
    ui::theme::*,
};
use gpui::{Context, Div, Stateful, div, prelude::*, px};

fn button(id: &'static str, label: impl Into<gpui::SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_md()
        .bg(color(HOVER))
        .cursor_pointer()
        .hover(|style| style.bg(color(SELECTED)))
        .child(label.into())
}

impl FileManager {
    pub(crate) fn operation_panel(&self, cx: &mut Context<Self>) -> Option<Div> {
        let progress = self.transfer_progress.as_ref()?;
        let state = progress.snapshot();
        let controlled = self
            .active_operation
            .as_ref()
            .is_some_and(|operation| operation.journal.is_some());
        let label = self.language.text(if self.queue_failed {
            "Operation needs attention"
        } else if state.paused {
            "Paused"
        } else {
            match state.phase {
                Phase::Preparing => "Preparing transfer…",
                Phase::SavingUndo => "Saving undo history…",
                Phase::Moving => "Moving…",
                Phase::Copying => "Copying…",
                Phase::Finishing => "Finishing transfer…",
            }
        });
        let mut panel = div()
            .absolute()
            .bottom(px(38.))
            .right(px(16.))
            .p_3()
            .rounded_lg()
            .bg(color(BACKGROUND))
            .border_1()
            .border_color(color(BORDER))
            .flex()
            .flex_col()
            .gap_2();
        if self.queue_background && state.conflict.is_none() && !self.queue_failed {
            return Some(
                panel.child(
                    button(
                        "show-operations",
                        format!("{label} · {}", self.operation_queue.len() + 1),
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.queue_background = false;
                        cx.notify();
                    })),
                ),
            );
        }
        panel = panel.w(px(380.)).max_w_full();
        let fraction = state
            .total
            .filter(|total| *total > 0)
            .map(|total| (state.completed as f32 / total as f32).clamp(0., 1.));
        panel = panel
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(label)
                    .children(fraction.map(|fraction| format!("{:.0}%", fraction * 100.))),
            )
            .child(
                div()
                    .w_full()
                    .h(px(6.))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(color(BORDER))
                    .child({
                        let bar = div()
                            .h_full()
                            .bg(color(ACCENT_BLUE))
                            .w(gpui::relative(fraction.unwrap_or(0.3)));
                        if fraction.is_none() && !state.paused && !self.queue_failed {
                            use gpui::{Animation, AnimationExt};
                            bar.with_animation(
                                "transfer-preparation",
                                Animation::new(std::time::Duration::from_secs(1))
                                    .repeat()
                                    .with_easing(gpui::pulsating_between(0.3, 1.)),
                                |bar, delta| bar.opacity(delta),
                            )
                            .into_any_element()
                        } else {
                            bar.into_any_element()
                        }
                    }),
            );
        if state.files > 0 {
            panel = panel
                .child(format!(
                    "{} / {} {}",
                    state.files_done,
                    state.files,
                    self.language.text("items")
                ))
                .child(format!(
                    "{} / {}",
                    self.language.size(Some(state.bytes_done)),
                    self.language.size(Some(state.bytes))
                ));
            if matches!(state.phase, Phase::Copying | Phase::Moving)
                && !state.paused
                && state.conflict.is_none()
                && let Some(remaining) = state.remaining
            {
                panel = panel.child(format!(
                    "~{} {}",
                    remaining.as_secs().max(1),
                    self.language.text("seconds remaining")
                ));
            }
        }
        if let Some(conflict) = &state.conflict {
            panel =
                panel
                    .child(div().text_color(color(ACCENT)).child(self.language.text(
                        if conflict.identical {
                            "Identical contents already exist"
                        } else {
                            "Destination name conflict"
                        },
                    )))
                    .child(div().text_size(px(11.)).child(format!(
                        "{}: {} ({})",
                        self.language.text("Source"),
                        conflict.source.display(),
                        self.language.size(Some(conflict.source_bytes))
                    )))
                    .child(div().text_size(px(11.)).child(format!(
                        "{}: {} ({})",
                        self.language.text("Destination"),
                        conflict.destination.display(),
                        self.language.size(Some(conflict.destination_bytes))
                    )))
                    .child(self.language.text(
                        "Existing contents are preserved. Keep both creates a numbered name.",
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                button("skip-conflict", self.language.text("Skip")).on_click(
                                    cx.listener(|view, _, _, cx| {
                                        if let Some(progress) = &view.transfer_progress {
                                            progress.resolve(Resolution::Skip, false);
                                        }
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                button("keep-conflict", self.language.text("Keep both")).on_click(
                                    cx.listener(|view, _, _, cx| {
                                        if let Some(progress) = &view.transfer_progress {
                                            progress.resolve(Resolution::KeepBoth, false);
                                        }
                                        cx.notify();
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                button("skip-all-conflicts", self.language.text("Skip all"))
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        if let Some(progress) = &view.transfer_progress {
                                            progress.resolve(Resolution::Skip, true);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                button("keep-all-conflicts", self.language.text("Keep all"))
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        if let Some(progress) = &view.transfer_progress {
                                            progress.resolve(Resolution::KeepBoth, true);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    );
        }
        if self.queue_failed {
            panel = panel
                .children(
                    self.error
                        .as_ref()
                        .map(|error| div().text_color(color(ERROR)).child(error.clone())),
                )
                .child(self.language.text(
                    "The journal and undo backups are retained. Resolve the problem, then retry.",
                ))
                .child(
                    button("retry-operation", self.language.text("Retry"))
                        .on_click(cx.listener(|view, _, _, cx| view.retry_operation(cx))),
                )
                .child(
                    button("cancel-failed-operation", self.language.text("Cancel"))
                        .on_click(cx.listener(|view, _, _, cx| view.cancel_failed_operation(cx))),
                );
        } else if controlled && state.phase != Phase::Finishing {
            panel = panel.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        button(
                            "pause-operation",
                            self.language
                                .text(if state.paused { "Resume" } else { "Pause" }),
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            if let Some(progress) = &view.transfer_progress {
                                progress.toggle_pause();
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        button("background-operation", self.language.text("Background")).on_click(
                            cx.listener(|view, _, _, cx| {
                                view.queue_background = true;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        button("cancel-operation", self.language.text("Cancel")).on_click(
                            cx.listener(|view, _, _, cx| {
                                if let Some(progress) = &view.transfer_progress {
                                    progress.cancel();
                                }
                                cx.notify();
                            }),
                        ),
                    ),
            );
        }
        let verifying = if controlled {
            progress.verification()
        } else {
            self.verify_transfers
        };
        panel = panel.child(
            button(
                "verify-transfers",
                format!(
                    "{} {}",
                    if verifying { "☑" } else { "☐" },
                    self.language.text("Verify copied contents (SHA-256)")
                ),
            )
            .on_click(cx.listener(|view, _, _, cx| {
                let enabled = view
                    .transfer_progress
                    .as_ref()
                    .map_or(view.verify_transfers, |progress| progress.verification());
                view.verify_transfers = !enabled;
                if let Some(progress) = &view.transfer_progress {
                    progress.set_verification(view.verify_transfers);
                }
                cx.notify();
            })),
        );
        if !self.operation_queue.is_empty() {
            panel =
                panel.child(self.language.text("Waiting operations")).child(
                    div()
                        .id("waiting-operations")
                        .max_h(px(140.))
                        .overflow_y_scroll()
                        .children(self.operation_queue.iter().enumerate().map(
                            |(index, operation)| {
                                let label = operation.journal.as_ref().map(queue::label).unwrap_or(
                                    match operation.operation {
                                        Operation::Transfer { .. } => "Preparing transfer…",
                                        _ => "Waiting operation",
                                    },
                                );
                                div()
                                    .id(("waiting-operation", index))
                                    .flex()
                                    .justify_between()
                                    .gap_2()
                                    .child(self.language.text(label))
                                    .child(
                                        div()
                                            .id(("cancel-waiting", index))
                                            .cursor_pointer()
                                            .px_2()
                                            .child("×")
                                            .on_click(cx.listener(move |view, _, _, cx| {
                                                view.cancel_waiting(index, cx)
                                            })),
                                    )
                            },
                        )),
                );
        }
        Some(panel)
    }
}
