mod files;

use files::{Entry, read_directory};
use gpui::{
    App, Application, Bounds, Context, Task, TitlebarOptions, UniformListScrollHandle, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size, uniform_list,
};
use std::{path::PathBuf, process::Command};

struct FileManager {
    path: PathBuf,
    home: PathBuf,
    entries: Vec<Entry>,
    hidden: bool,
    loading: bool,
    error: Option<String>,
    listing: Option<Task<()>>,
    scroll: UniformListScrollHandle,
}

impl FileManager {
    fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let mut view = Self {
            path: path.clone(),
            home,
            entries: Vec::new(),
            hidden: false,
            loading: false,
            error: None,
            listing: None,
            scroll: UniformListScrollHandle::new(),
        };
        view.navigate(path, cx);
        view
    }

    fn navigate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        let hidden = self.hidden;
        let requested = path.clone();
        let read = cx
            .background_executor()
            .spawn(async move { read_directory(&requested, hidden) });
        // Replacing this task cancels an outdated navigation request.
        self.listing = Some(cx.spawn(async move |view, cx| {
            let result = read.await;
            let _ = view.update(cx, |view, cx| {
                view.loading = false;
                match result {
                    Ok(entries) => {
                        if view.path != path {
                            view.scroll = UniformListScrollHandle::new();
                        }
                        view.path = path;
                        view.entries = entries;
                    }
                    Err(error) => {
                        view.error = Some(format!("Cannot read {}: {error}", path.display()))
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn open(&mut self, entry: Entry, cx: &mut Context<Self>) {
        if entry.directory {
            self.navigate(entry.path, cx);
            return;
        }
        let open = cx.background_executor().spawn(async move {
            Command::new("xdg-open")
                .arg(&entry.path)
                .output()
                .map_err(|error| format!("Cannot open {}: {error}", entry.path.display()))
                .and_then(|output| {
                    if output.status.success() {
                        Ok(())
                    } else {
                        Err(format!(
                            "Cannot open {}: {}",
                            entry.path.display(),
                            String::from_utf8_lossy(&output.stderr).trim()
                        ))
                    }
                })
        });
        cx.spawn(async move |view, cx| {
            if let Err(error) = open.await {
                let _ = view.update(cx, |view, cx| {
                    view.error = Some(error);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

fn button(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_3()
        .py_2()
        .bg(rgb(0xdddddd))
        .rounded_sm()
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0xcccccc)))
        .child(label)
}

impl Render for FileManager {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = if self.loading {
            "Loading…".to_string()
        } else {
            format!(
                "{} items · Click a folder to browse or a file to open",
                self.entries.len()
            )
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xf5f5f5))
            .text_color(rgb(0x222222))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .p_3()
                    .child(button("up", "Up").on_click(cx.listener(|view, _, _, cx| {
                        if let Some(parent) = view.path.parent() {
                            view.navigate(parent.to_path_buf(), cx);
                        }
                    })))
                    .child(
                        button("home", "Home").on_click(cx.listener(|view, _, _, cx| {
                            view.navigate(view.home.clone(), cx);
                        })),
                    )
                    .child(
                        button("root", "Root").on_click(cx.listener(|view, _, _, cx| {
                            view.navigate(PathBuf::from("/"), cx);
                        })),
                    )
                    .child(
                        button("refresh", "Refresh").on_click(cx.listener(|view, _, _, cx| {
                            view.navigate(view.path.clone(), cx);
                        })),
                    )
                    .child(
                        button(
                            "hidden",
                            if self.hidden {
                                "Hide hidden files"
                            } else {
                                "Show hidden files"
                            },
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.hidden = !view.hidden;
                            view.navigate(view.path.clone(), cx);
                        })),
                    ),
            )
            .child(div().px_3().pb_2().child(self.path.display().to_string()))
            .children(
                self.error
                    .as_ref()
                    .map(|error| div().p_3().text_color(rgb(0xaa2222)).child(error.clone())),
            )
            .child(
                uniform_list(
                    "files",
                    self.entries.len(),
                    cx.processor(|view, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let entry = view.entries[index].clone();
                                let kind = if entry.directory { "Folder" } else { "File" };
                                let bytes = entry
                                    .bytes
                                    .map(|bytes| format!("{bytes} B"))
                                    .unwrap_or_default();
                                div()
                                    .id(index)
                                    .h(px(32.))
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(0xe0e0e0)))
                                    .child(div().w(px(60.)).child(kind))
                                    .child(
                                        div().flex_1().overflow_hidden().child(entry.name.clone()),
                                    )
                                    .child(bytes)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.open(entry.clone(), cx)
                                    }))
                            })
                            .collect()
                    }),
                )
                .track_scroll(self.scroll.clone())
                .flex_1()
                .min_h_0(),
            )
            .child(div().p_3().child(
                if !self.loading && self.entries.is_empty() && self.error.is_none() {
                    "This folder is empty".to_string()
                } else {
                    status
                },
            ))
    }
}

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/"));
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let path = path.canonicalize().unwrap_or(path);
    Application::new().run(move |cx: &mut App| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(900.), px(600.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Virial — File Manager".into()),
                ..Default::default()
            }),
            app_id: Some("virial-gpui".into()),
            window_min_size: Some(size(px(650.), px(300.))),
            ..Default::default()
        };
        if let Err(error) = cx.open_window(options, |_, cx| cx.new(|cx| FileManager::new(path, cx)))
        {
            eprintln!("Cannot open Virial: {error}");
            cx.quit();
        }
        cx.activate(true);
    });
}
