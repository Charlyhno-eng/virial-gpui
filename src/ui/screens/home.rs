use crate::{
    app::FileManager,
    ui::icons::{file_icon, icon},
    ui::theme::*,
};
use gpui::{Animation, AnimationExt, Context, Div, FontWeight, div, prelude::*, px, uniform_list};
use std::time::Duration;

impl FileManager {
    /// How many sessions the details panel lists before stopping: the newest
    /// ones are enough to recognise an application, and the panel must stay
    /// readable on a home with hundreds of conversations.
    const MAX_SESSIONS: usize = 5;

    /// Extended details belong to the explicit Properties dialog, never the preview.
    pub(crate) fn details_panel(&self, entry: &crate::domain::models::Entry) -> Div {
        let package = crate::infrastructure::packages::inspect(&entry.path);
        let harness = entry
            .directory
            .then(|| crate::infrastructure::harness::inspect(&entry.path))
            .flatten();
        let toolkit = harness
            .as_ref()
            .map(|harness| crate::infrastructure::toolkit::toolkit_with(&harness.home))
            .unwrap_or_default();
        let is_previewed = self.preview_path.as_ref() == Some(&entry.path);
        let name = entry.name.clone();
        let path = entry.path.display().to_string();
        let kind = entry.kind();
        let size = self.language.size(entry.bytes);
        let symbol = entry.icon();
        div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_3()
            .px_4()
            .pt_5()
            .pb_4()
            .bg(translucent(SURFACE, 0.16))
            .child(file_icon(symbol, 34.))
            .child(div().text_size(px(14.)).text_ellipsis().child(name))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(color(MUTED))
                    .text_ellipsis()
                    .child(path),
            )
            .child(div().h(px(1.)).my_1().bg(color(BORDER)))
            .when(is_previewed, |panel| {
                if let crate::state::preview::Preview::Folder(entries) = &self.preview {
                    panel.child(
                        div()
                            .text_size(px(11.))
                            .child(self.language.item_count(entries.len())),
                    )
                } else {
                    panel
                }
            })
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.))
                    .child(self.language.text("Type"))
                    .child(
                        div()
                            .text_color(color(MUTED))
                            .child(self.language.text(kind)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.))
                    .child(self.language.text("Size"))
                    .child(div().text_color(color(MUTED)).child(size)),
            )
            .when_some(package.clone(), |panel, package| {
                // The package-host block, styled like the image metadata one.
                let role = self.language.text(match package.role {
                    crate::infrastructure::packages::Role::Manifest => "Package manifest",
                    crate::infrastructure::packages::Role::Lock => "Package lock file",
                    crate::infrastructure::packages::Role::Credentials => "Package credentials",
                });
                panel
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Registry"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(file_icon(package.icon, 14.))
                                    .child(div().text_color(color(MUTED)).child(format!(
                                        "{} · {}",
                                        self.language.text(package.host),
                                        package.registry
                                    ))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Role"))
                            .child(div().text_color(color(MUTED)).child(role)),
                    )
                    .children(package.version.as_ref().map(|version| {
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Version"))
                            .child(div().text_color(color(MUTED)).child(version.clone()))
                    }))
                    .when(package.holds_credentials, |panel| {
                        panel.child(
                            div()
                                .flex()
                                .gap_1()
                                .items_center()
                                .text_size(px(11.))
                                .text_color(color(ERROR))
                                .child(icon("info", 12., ERROR))
                                .child(
                                    self.language
                                        .text("This file carries an access key for the registry"),
                                ),
                        )
                    })
            })
            .when_some(harness.clone(), |panel, harness| {
                // The harness block: which agentic CLI owns this folder, its
                // home, its system prompt and its credential files.
                let role = self.language.text(match harness.role {
                    crate::infrastructure::harness::Role::Home => "Harness home",
                    crate::infrastructure::harness::Role::Folder => "Harness folder",
                });
                let kind = self.language.text(harness.kind.label());
                panel
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Harness"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(file_icon(harness.icon, 14.))
                                    .child(div().text_color(color(MUTED)).child(harness.name)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Role"))
                            .child(div().text_color(color(MUTED)).child(role)),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Kind"))
                            .child(div().text_color(color(MUTED)).child(kind)),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Home"))
                            .child(
                                div()
                                    .text_color(color(MUTED))
                                    .text_ellipsis()
                                    .child(harness.home.display().to_string()),
                            ),
                    )
                    .children(harness.prompt.as_ref().map(|prompt| {
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(11.))
                            .child(self.language.text("Prompt"))
                            .child(
                                div()
                                    .text_color(color(MUTED))
                                    .text_ellipsis()
                                    .child(prompt.display().to_string()),
                            )
                    }))
                    .when(!harness.credentials.is_empty(), |panel| {
                        panel.child(
                            div()
                                .flex()
                                .gap_1()
                                .items_center()
                                .text_size(px(11.))
                                .text_color(color(MUTED))
                                .child(icon("info", 12., ACCENT))
                                .child(format!(
                                    "{}: {}",
                                    self.language.text("Credentials"),
                                    harness.credentials.join(", ")
                                )),
                        )
                    })
                    .children(
                        toolkit
                            .sessions
                            .iter()
                            .take(Self::MAX_SESSIONS)
                            .map(|session| {
                                div()
                                    .flex()
                                    .justify_between()
                                    .gap_2()
                                    .text_size(px(11.))
                                    .child(self.language.text("Session"))
                                    .child(
                                        div()
                                            .flex()
                                            .min_w_0()
                                            .flex_col()
                                            .items_end()
                                            .text_color(color(MUTED))
                                            .child(session.name.clone())
                                            .child(
                                                session
                                                    .cwd
                                                    .as_ref()
                                                    .map(|cwd| cwd.display().to_string())
                                                    .unwrap_or_default(),
                                            ),
                                    )
                            }),
                    )
                    .when(!toolkit.skills.is_empty(), |panel| {
                        panel.child(
                            div()
                                .flex()
                                .justify_between()
                                .gap_2()
                                .text_size(px(11.))
                                .child(self.language.text("Skills"))
                                .child(
                                    div()
                                        .flex()
                                        .min_w_0()
                                        .flex_col()
                                        .items_end()
                                        .text_color(color(MUTED))
                                        .child(toolkit.skills.len().to_string())
                                        .child(
                                            toolkit
                                                .skills
                                                .iter()
                                                .map(|skill| skill.name.as_str())
                                                .collect::<Vec<_>>()
                                                .join(", "),
                                        ),
                                ),
                        )
                    })
                    .when(!toolkit.servers.is_empty(), |panel| {
                        panel.children(toolkit.servers.iter().map(|server| {
                            div()
                                .flex()
                                .justify_between()
                                .gap_2()
                                .text_size(px(11.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(self.language.text("MCP server"))
                                        .child(file_icon(
                                            crate::infrastructure::toolkit::MCP_ICON,
                                            12.,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .min_w_0()
                                        .flex_col()
                                        .items_end()
                                        .text_color(color(MUTED))
                                        .child(server.name.clone())
                                        .child(server.transport.clone().unwrap_or_default()),
                                )
                        }))
                    })
            })
            .when_some(
                is_previewed
                    .then(|| self.preview.image_metadata())
                    .flatten(),
                |panel, metadata| {
                    panel.children(
                        [
                            ("Format", metadata.format.clone()),
                            (
                                "Dimensions",
                                format!("{} × {} px", metadata.width, metadata.height),
                            ),
                        ]
                        .into_iter()
                        .map(|(label, value)| {
                            div()
                                .flex()
                                .justify_between()
                                .gap_3()
                                .text_size(px(11.))
                                .child(self.language.text(label))
                                .child(div().text_color(color(MUTED)).child(value))
                        }),
                    )
                },
            )
    }

    pub(crate) fn file_list(&self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let title = self.location.title(&self.home, self.language);
        let visible_count = self.entries.len();
        let scroll = self.scroll.0.borrow().base_handle.clone();
        let rectangle = self
            .marquee
            .as_ref()
            .map(|marquee| (marquee.start, marquee.end));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .id("file-list-area")
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|view, event: &gpui::MouseDownEvent, window, cx| {
                    view.show_menu(None, event.position, window, cx);
                }),
            )
            .when_some(self.location.directory().map(|path| path.to_path_buf()), |area, directory| {
                self.drop_target(area, directory, cx)
            })
            .min_w_0()
            .min_h_0()
            .when(self.details_open && !self.preview_history.is_empty(), |area| {
                area.min_w(px(180.))
            })
            .child(
                div()
                    .px_4()
                    .pt_3()
                    .pb_3()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_size(px(17.))
                            .text_ellipsis()
                            .font_weight(FontWeight::SEMIBOLD)
                            .flex().items_center().gap_2()
                            .child(icon(self.location.icon(), 26., ACCENT_BLUE))
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.))
                            .text_color(color(MUTED))
                            .text_ellipsis()
                            .child(self.location.description(self.language)),
                    ),
            )
            .children(self.error.as_ref().map(|error| {
                div()
                    .mx_4()
                    .mb_3()
                    .p_3()
                    .rounded_md()
                    .bg(color(ERROR_BG))
                    .text_size(px(11.))
                    .text_color(color(ERROR))
                    .child(error.clone())
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(28.))
                    .flex_shrink_0()
                    .gap_3()
                    .px_4()
                    .border_b_1()
                    .border_color(color(BORDER))
                    .text_size(px(10.))
                    .text_color(color(MUTED))
                    .child(div().id("sort-name").flex_1().min_w_0().flex().items_center().gap_2().cursor_pointer()
                        .hover(|style| style.text_color(color(TEXT)))
                        .child(self.language.text("NAME"))
                        .child(icon(if self.name_descending { "down" } else { "up" }, 11., MUTED))
                        .on_click(cx.listener(|view, _, _, cx| view.toggle_name_sort(cx))))
                    .child(
                        div()
                            .w(px(76.))
                            .text_right()
                            .child(self.language.text("SIZE")),
                    )
                    .child(div().w(px(150.)).flex_shrink_0().child(self.language.text("MODIFIED"))),
            )
            .child(if visible_count == 0 {
                let (heading, description) = if self.loading {
                    ("Loading folder…", "Reading directory contents")
                } else if self.error.is_some() {
                    (
                        "Folder unavailable",
                        "Choose another location or try refreshing",
                    )
                } else if self.location == crate::domain::location::Location::Trash {
                    ("Trash is empty", "Deleted items appear here")
                } else if matches!(self.location, crate::domain::location::Location::Tag(_)) {
                    ("No tagged items", "Right-click a file or folder and choose Tags to add a color")
                } else if self.location == crate::domain::location::Location::Recent {
                    (
                        "No recent files",
                        "Files opened with Virial and desktop applications appear here",
                    )
                } else {
                    (
                        "This folder is empty",
                        "Create a file or folder using the context menu",
                    )
                };
                div()
                    .flex_1()
                    .min_h_0()
                    .px_4()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .gap_3()
                    .child(icon(self.location.icon(), 40., ACCENT))
                    .child(div().text_size(px(13.)).child(self.language.text(heading)))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(color(MUTED))
                            .child(self.language.text(description)),
                    )
                    .into_any_element()
            } else {
                let scrollbar = crate::ui::components::scrollbar::render(
                    &self.scroll,
                    visible_count,
                    gpui::rgba(0xff000000),
                );
                div().relative().flex().flex_col().flex_1().min_h_0().overflow_hidden()
                .child(uniform_list(
                    self.location.id(),
                    visible_count,
                    cx.processor(|view, range: std::ops::Range<usize>, _, cx| {
                        let paths = view.selected_paths().into();
                        range
                            .map(|index| {
                                let entry = &view.entries[index];
                                let row_path: std::sync::Arc<std::path::Path> = entry.path.as_path().into();
                                let recent = matches!(view.location,
                                    crate::domain::location::Location::Recent | crate::domain::location::Location::Tag(_));
                                let row_height = if recent {
                                    crate::ui::theme::RECENT_ROW_HEIGHT
                                } else {
                                    ROW_HEIGHT
                                };
                                let selected = view.selection.indices.contains(&index);
                                let payload = view.drag_payload(index, &paths);
                                let trash = view.location == crate::domain::location::Location::Trash;
                                let directory = (entry.browsable() && !trash).then(|| entry.path.clone());
                                let rename_input = view.rename.as_ref()
                                    .filter(|rename| rename.source == entry.path)
                                    .map(|rename| rename.input.clone());
                                let renaming = rename_input.is_some();
                                let name = if let Some(input) = rename_input {
                                    div().flex_1().min_w_0()
                                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                        .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                                        .on_mouse_down_out(cx.listener(|view, _, window, cx| {
                                            if view.rename.is_some() {
                                                view.cancel_rename(window, cx);
                                            }
                                        }))
                                        .child(input)
                                } else if recent {
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .justify_center()
                                        .gap_1()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .text_ellipsis()
                                                .text_size(px(12.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(entry.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .min_w_0()
                                                .text_size(px(10.))
                                                .text_color(color(MUTED))
                                                .text_ellipsis()
                                                .child(entry.path.display().to_string()),
                                        )
                                } else {
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .text_size(px(11.))
                                        .child(entry.name.clone())
                                };
                                let row = div()
                                    .id(row_path.clone())
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(row_height))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .cursor_pointer()
                                    .border_l_2()
                                    .border_color(if selected {
                                        translucent(ACCENT_BLUE, 0.28)
                                    } else {
                                        gpui::transparent_black()
                                    })
                                    .when(selected, |row| row.bg(translucent(ACCENT_BLUE, 0.08)))
                                    .when(!selected && index % 2 != 0, |row| {
                                        row.bg(translucent(SIDEBAR, 0.22))
                                    })
                                    .hover(|style| {
                                        style.bg(if selected { translucent(ACCENT_BLUE, 0.11) } else { color(HOVER) })
                                    })
                                    .child(file_icon(entry.icon(), 19.))
                                    .child(name)
                                    .when(!trash && view.location.remote().is_none() && !view.tags.colors(&entry.path).is_empty(), |row| {
                                        row.child(div().flex().items_center().gap_1().children(
                                            view.tags.colors(&entry.path).iter().map(|tag| {
                                                div().size(px(7.)).flex_shrink_0().rounded_full().bg(color(tag.color()))
                                            })
                                        ))
                                    })
                                    .child(
                                        div()
                                            .w(px(76.))
                                            .flex_shrink_0()
                                            .text_right()
                                            .text_size(px(11.))
                                            .text_color(color(MUTED))
                                            .child(view.language.size(entry.bytes)),
                                    )
                                    .child(div().w(px(150.)).flex_shrink_0().text_size(px(10.)).text_color(color(MUTED))
                                        .child(view.entry_modified.get(&entry.path).map(|date| date.display()).unwrap_or_else(|| "—".into())))
                                    .on_mouse_down(gpui::MouseButton::Right, {
                                        let row_path = row_path.clone();
                                        cx.listener(
                                            move |view,
                                                  event: &gpui::MouseDownEvent,
                                                  window,
                                                  cx| {
                                                cx.stop_propagation();
                                                let entry = view.entries.get(index)
                                                    .filter(|entry| entry.path == row_path.as_ref())
                                                    .cloned();
                                                let Some(entry) = entry else { return; };
                                                view.show_menu(
                                                    Some(entry),
                                                    event.position,
                                                    window,
                                                    cx,
                                                );
                                            },
                                        )
                                    })
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(
                                        move |view, event: &gpui::MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            if view.busy || view.loading || renaming { return; }
                                            view.focus.focus(window);
                                            view.cancel_pending_preview();
                                            if !event.modifiers.control && !event.modifiers.shift {
                                                view.details_open = false;
                                                view.preview_expanded = false;
                                            }
                                            view.selection.pointer_down(index, event.modifiers.control, event.modifiers.shift);
                                            cx.notify();
                                        },
                                    ))
                                    .when(!renaming && !view.busy && !view.loading && view.marquee.is_none(), |row| {
                                        let manager = cx.entity();
                                        row.when(!trash, |row| row.on_drag(payload, move |drag, _, window, cx| {
                                            if let Some(uris) = crate::platform::file_drag::uri_list(&drag.paths) {
                                                window.start_file_drag(uris);
                                            }
                                            manager.update(cx, |view, cx| {
                                                view.cancel_pending_preview();
                                                view.details_open = false;
                                                view.preview_expanded = false;
                                                view.menu = None;
                                                cx.notify();
                                            });
                                            cx.new(|_| drag.clone())
                                        }))
                                        .on_click(cx.listener(
                                            move |view, event: &gpui::ClickEvent, _, cx| {
                                                if !event.standard_click() || event.is_keyboard() { return; }
                                                let click_count = event.click_count();
                                                if click_count != 1 {
                                                    view.cancel_pending_preview();
                                                }
                                                cx.stop_propagation();
                                                // Use the modifiers from button-down, even when
                                                // Ctrl is released before the mouse button.
                                                let modifiers = match event {
                                                    gpui::ClickEvent::Mouse(event) => event.down.modifiers,
                                                    gpui::ClickEvent::Keyboard(_) => gpui::Modifiers::default(),
                                                };
                                                view.selection.pointer_click(index, modifiers.control, modifiers.shift);
                                                if !modifiers.control && !modifiers.shift {
                                                    view.details_open = false;
                                                    view.preview_expanded = false;
                                                    if click_count >= 2
                                                        && let Some(entry) = view.entries.get(index)
                                                            .filter(|entry| entry.path == row_path.as_ref())
                                                            .cloned()
                                                        {
                                                            view.open(entry, cx);
                                                    }
                                                }
                                                if click_count == 1 && !modifiers.control && !modifiers.shift && view.selection.indices.len() == 1 {
                                                    let path = row_path.clone();
                                                    view.schedule_preview(cx, move |view, cx| {
                                                        if view.selection.indices.len() == 1
                                                            && view.selection.primary()
                                                                .and_then(|index| view.entries.get(index))
                                                                .is_some_and(|entry| entry.path == path.as_ref())
                                                        {
                                                            view.show_preview(cx);
                                                        }
                                                    });
                                                }
                                                cx.notify();
                                            },
                                        ))
                                    });
                                let row = if let Some(directory) = directory {
                                    view.drop_target(row, directory, cx)
                                } else { row };
                                div().w_full().h(px(row_height)).flex()
                                    .child(div().w(px(14.)).h_full().flex_shrink_0())
                                    // Selection changes on mouse-down must preserve the
                                    // row's pending drag gesture across the next render.
                                    .child(row.with_animation(
                                        "selection-light",
                                        Animation::new(Duration::from_millis(150)).with_easing(gpui::ease_out_quint()),
                                        move |row, delta| {
                                            if selected { row.bg(translucent(ACCENT_BLUE, 0.04 + 0.04 * delta)) } else { row }
                                        },
                                    ))
                            })
                            .collect()
                    }),
                )
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::begin_marquee))
                .w_full()
                .track_scroll(self.scroll.clone())
                .flex_1()
                .min_h_0())
                .children(rectangle.map(|(start, end)| {
                    gpui::canvas(move |_, _, _| {
                        let offset = scroll.offset();
                        let bounds = scroll.bounds();
                        let top_left = gpui::point(start.x.min(end.x), start.y.min(end.y)) + bounds.origin + offset;
                        let size = gpui::size((end.x - start.x).abs(), (end.y - start.y).abs());
                        gpui::Bounds::new(top_left, size)
                    }, |_, bounds, window, _| {
                        window.paint_quad(gpui::quad(bounds, px(0.), translucent(ACCENT_BLUE, 0.15), px(1.), color(ACCENT_BLUE), gpui::BorderStyle::Solid));
                    }).absolute().size_full()
                }))
                .child(scrollbar)
                .into_any_element()
            })
            .child(
                div()
                    .h(px(30.))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(color(BORDER))
                    .text_size(px(10.))
                    .text_color(color(MUTED))
                    .when(self.busy || self.loading, |bar| {
                        bar.child(
                            div()
                                .size(px(5.))
                                .rounded_full()
                                .bg(color(ACCENT_BLUE))
                                .with_animation(
                                    "activity",
                                    Animation::new(Duration::from_millis(1200))
                                        .repeat()
                                        .with_easing(gpui::pulsating_between(0.4, 1.)),
                                    |dot, delta| dot.opacity(delta),
                                ),
                        )
                    })
                    .child(if self.busy {
                        self.language.text("Working…").into()
                    } else if self.loading {
                        self.language.text("Loading…").into()
                    } else {
                        self.language
                            .counts(self.folder_count, self.entries.len() - self.folder_count)
                    })
                    .child(div().flex_1().min_w_0().text_ellipsis().text_color(color(TEXT))
                        .child(if self.selection.indices.len() > 1 {
                            format!(" · {}", self.language.selected_count(self.selection.indices.len()))
                        } else {
                            self.selection.primary().and_then(|index| self.entries.get(index))
                                .map(|entry| format!(" · {}", entry.name)).unwrap_or_default()
                        }))
                    .child(self.language.text("Double-click to open · Enter")),
            )
    }
}
