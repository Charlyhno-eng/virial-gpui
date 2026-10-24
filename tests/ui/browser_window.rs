use super::*;
use gpui::{TestAppContext, WindowHandle};
use std::time::Duration;

fn settle(cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
}

#[test]
fn context_menu_keeps_properties_visible_at_the_bottom_of_the_window() {
    let (_temp, mut cx, window) = setup();
    for split in [false, true] {
        window
            .update(&mut cx, |view, window, cx| {
                if split {
                    view.toggle_split(window, cx);
                }
            })
            .unwrap();
        settle(&mut cx);
        for height in [580., 360.] {
            cx.simulate_window_resize(window.into(), gpui::size(px(900.), px(height)));
            window
                .update(&mut cx, |view, _, cx| {
                    view.active().update(cx, |pane, cx| {
                        pane.selection.click(0, false, false);
                        pane.menu = Some(crate::state::actions::Menu {
                            position: gpui::point(px(850.), px(height - 5.)),
                            entry: Some(pane.entries[0].clone()),
                            compression_open: false,
                            tags_open: false,
                        });
                        cx.notify();
                    });
                })
                .unwrap();
            settle(&mut cx);
            let mut visual = gpui::VisualTestContext::from_window(window.into(), &cx);
            let menu = visual.debug_bounds("context-menu").unwrap();
            assert!(menu.bottom() <= px(height));
            assert!(menu.right() <= px(900.));
            if height == 360. {
                visual.simulate_event(gpui::ScrollWheelEvent {
                    position: menu.center(),
                    delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-1000.))),
                    ..Default::default()
                });
                visual.run_until_parked();
            }
            {
                let properties = visual.debug_bounds("context-action-Properties").unwrap();
                assert!(properties.bottom() <= menu.bottom());
                visual.simulate_click(properties.center(), Default::default());
                window
                    .update(&mut cx, |view, _, cx| {
                        assert!(view.active().read(cx).dialog.is_some());
                        view.active().update(cx, |pane, cx| {
                            pane.dialog = None;
                            cx.notify();
                        });
                    })
                    .unwrap();
            }
        }
    }
    cx.quit();
}

fn setup() -> (
    tempfile::TempDir,
    TestAppContext,
    WindowHandle<BrowserWindow>,
) {
    let temp = tempfile::tempdir().unwrap();
    let files = temp.path().join("files");
    std::fs::create_dir_all(files.join("alpha")).unwrap();
    std::fs::create_dir_all(files.join("beta")).unwrap();
    for index in 0..30 {
        std::fs::write(files.join(format!("file-{index:02}.txt")), "original\n").unwrap();
    }
    let home = temp.path().to_path_buf();
    let data = home.join("data");
    let mut cx = TestAppContext::single();
    let window = cx.add_window(|window, cx| {
        let pane = cx.new(|cx| FileManager::with_paths(files, home, data, false, window, cx));
        BrowserWindow::from_pane(pane, window, cx)
    });
    settle(&mut cx);
    (temp, cx, window)
}

#[test]
fn tab_shortcuts_keep_location_history_selection_and_view() {
    let (temp, mut cx, window) = setup();
    window
        .update(&mut cx, |view, _, cx| {
            view.active().update(cx, |pane, cx| {
                assert_eq!(pane.entries.len(), 32);
                pane.selection.click(3, false, false);
                pane.set_view_mode(ViewMode::Grid, cx);
            });
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "ctrl-t");
    settle(&mut cx);
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.tabs.items.len(), 2);
            assert_eq!(view.tabs.active, 1);
            view.active().update(cx, |pane, cx| {
                pane.set_view_mode(ViewMode::List, cx);
                pane.navigate(temp.path().join("files/alpha"), cx);
            });
        })
        .unwrap();
    settle(&mut cx);
    cx.simulate_keystrokes(window.into(), "ctrl-shift-tab");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.tabs.active, 0);
            let pane = view.active();
            let pane = pane.read(cx);
            assert_eq!(
                pane.location.directory().unwrap(),
                temp.path().join("files")
            );
            assert_eq!(pane.selection.primary(), Some(3));
            assert_eq!(pane.view_mode, ViewMode::Grid);
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "ctrl-tab");
    window
        .update(&mut cx, |view, _, cx| {
            let pane = view.active();
            let pane = pane.read(cx);
            assert_eq!(
                pane.location.directory().unwrap(),
                temp.path().join("files/alpha")
            );
            assert!(pane.history.back().is_some());
            assert_eq!(pane.view_mode, ViewMode::List);
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "ctrl-w");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.tabs.items.len(), 1))
        .unwrap();
    cx.simulate_keystrokes(window.into(), "ctrl-w");
    window
        .update(&mut cx, |view, _, _| assert_eq!(view.tabs.items.len(), 1))
        .unwrap();
    cx.quit();
}

#[test]
fn split_panes_copy_files_refresh_source_and_restore_when_toggled() {
    let (temp, mut cx, window) = setup();
    cx.simulate_keystrokes(window.into(), "ctrl-4");
    settle(&mut cx);
    window
        .update(&mut cx, |view, _, cx| {
            let tab = &view.tabs.items[0];
            assert!(tab.split && tab.right_active);
            assert_ne!(tab.left, *tab.right.as_ref().unwrap());
            tab.right.as_ref().unwrap().update(cx, |pane, cx| {
                pane.navigate(temp.path().join("files/alpha"), cx)
            });
        })
        .unwrap();
    settle(&mut cx);
    cx.simulate_keystrokes(window.into(), "ctrl-6");
    window
        .update(&mut cx, |view, _, cx| {
            assert!(!view.tabs.items[0].right_active);
            view.active().update(cx, |pane, cx| {
                let index = pane
                    .entries
                    .iter()
                    .position(|entry| entry.name == "file-00.txt")
                    .unwrap();
                pane.selection.click(index, false, false);
                pane.set_view_mode(ViewMode::Grid, cx);
            });
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "ctrl-c ctrl-6 ctrl-v");
    settle(&mut cx);
    assert_eq!(
        std::fs::read_to_string(temp.path().join("files/alpha/file-00.txt")).unwrap(),
        "original\n"
    );
    assert!(temp.path().join("files/file-00.txt").exists());
    window
        .update(&mut cx, |view, _, cx| {
            let tab = &view.tabs.items[0];
            assert_eq!(tab.right.as_ref().unwrap().read(cx).entries.len(), 1);
            assert_eq!(tab.left.read(cx).view_mode, ViewMode::Grid);
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "ctrl-4 ctrl-4");
    window
        .update(&mut cx, |view, _, cx| {
            assert!(view.tabs.items[0].split);
            assert_eq!(
                view.active().read(cx).location.directory().unwrap(),
                temp.path().join("files/alpha")
            );
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "ctrl-6");
    window
        .update(&mut cx, |view, _, cx| {
            view.active().update(cx, |pane, _| {
                let index = pane
                    .entries
                    .iter()
                    .position(|entry| entry.name == "file-01.txt")
                    .unwrap();
                pane.selection.click(index, false, false);
            });
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "ctrl-x ctrl-6 ctrl-v");
    settle(&mut cx);
    assert!(!temp.path().join("files/file-01.txt").exists());
    assert!(temp.path().join("files/alpha/file-01.txt").exists());
    window
        .update(&mut cx, |view, _, cx| {
            assert!(view.active().read(cx).clipboard.borrow().is_none());
            assert_eq!(view.tabs.items[0].left.read(cx).entries.len(), 31);
        })
        .unwrap();
    cx.quit();
}

#[test]
fn closing_tab_or_window_checks_busy_and_dirty_inactive_panes() {
    let (_temp, mut cx, window) = setup();
    cx.simulate_keystrokes(window.into(), "ctrl-4 ctrl-t");
    window
        .update(&mut cx, |view, window, cx| {
            let blocked = view.tabs.items[0].right.as_ref().unwrap().clone();
            blocked.update(cx, |pane, _| pane.busy = true);
            view.close_tab(0, window, cx);
            assert_eq!(view.tabs.items.len(), 2);
            assert_eq!(view.tabs.active, 0);
            assert!(view.tabs.items[0].right_active);
            assert!(blocked.read(cx).error.is_some());
            blocked.update(cx, |pane, cx| {
                pane.busy = false;
                let path = pane.home.join("editor.txt");
                let mut editor = crate::state::editor::Editor::new(
                    "original",
                    crate::infrastructure::editor_io::decode(b"original")
                        .unwrap()
                        .1,
                );
                editor.insert("changed");
                pane.attach_editor(editor, path, window, cx);
            });
            view.select_tab(1, window, cx);
            assert!(!view.can_close(0, window, cx));
            assert_eq!(view.tabs.active, 0);
            assert!(blocked.read(cx).editor_is_dirty(cx));
        })
        .unwrap();
    cx.quit();
}

#[test]
fn grid_keyboard_changes_rows_and_all_view_shortcuts_render() {
    let (_temp, mut cx, window) = setup();
    cx.simulate_keystrokes(window.into(), "ctrl-2");
    settle(&mut cx);
    window
        .update(&mut cx, |view, _, cx| {
            let pane = view.active();
            let pane = pane.read(cx);
            assert_eq!(pane.view_mode, ViewMode::Grid);
            assert!(pane.grid_columns > 1);
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "home down");
    window
        .update(&mut cx, |view, _, cx| {
            let pane = view.active();
            let pane = pane.read(cx);
            assert_eq!(pane.selection.primary(), Some(pane.grid_columns));
        })
        .unwrap();
    cx.simulate_keystrokes(window.into(), "ctrl-1 ctrl-2 ctrl-4");
    settle(&mut cx);
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.tabs.items[0].left.read(cx).view_mode, ViewMode::Grid);
            assert!(view.tabs.items[0].split);
        })
        .unwrap();
    cx.quit();
}

#[test]
fn divider_resizes_and_mouse_focus_tracks_the_clicked_pane() {
    let (_temp, mut cx, window) = setup();
    cx.simulate_window_resize(window.into(), gpui::size(px(900.), px(580.)));
    cx.simulate_keystrokes(window.into(), "ctrl-4");
    settle(&mut cx);
    let mut visual = gpui::VisualTestContext::from_window(window.into(), &cx);
    let left = visual.debug_bounds("left-pane").unwrap();
    let right = visual.debug_bounds("right-pane").unwrap();
    assert!(left.size.width > px(200.) && right.size.width > px(200.));
    visual.simulate_click(left.center(), Default::default());
    window
        .update(&mut cx, |view, window, cx| {
            assert!(!view.tabs.items[0].right_active);
            assert!(view.active().read(cx).focus.is_focused(window));
        })
        .unwrap();
    visual.simulate_click(right.center(), Default::default());
    window
        .update(&mut cx, |view, window, cx| {
            assert!(view.tabs.items[0].right_active);
            assert!(view.active().read(cx).focus.is_focused(window));
        })
        .unwrap();
    let divider = visual.debug_bounds("pane-divider").unwrap().center();
    visual.simulate_mouse_down(divider, MouseButton::Left, Default::default());
    let moved = divider + gpui::point(px(100.), px(0.));
    visual.simulate_mouse_move(moved, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(moved, MouseButton::Left, Default::default());
    settle(&mut cx);
    window
        .update(&mut cx, |view, _, _| {
            assert!(view.tabs.items[0].ratio > 0.6);
            assert!(!view.resizing);
        })
        .unwrap();
    cx.simulate_window_resize(window.into(), gpui::size(px(720.), px(420.)));
    settle(&mut cx);
    let left = visual.debug_bounds("left-pane").unwrap();
    let right = visual.debug_bounds("right-pane").unwrap();
    assert!(left.size.width > px(0.) && right.size.width > px(0.));
    cx.quit();
}

#[test]
fn filters_belong_to_their_pane() {
    let (_temp, mut cx, window) = setup();
    cx.simulate_keystrokes(window.into(), "ctrl-4");
    settle(&mut cx);
    window
        .update(&mut cx, |view, _, cx| {
            let input = view.active().read(cx).extension_input.clone();
            input.update(cx, |input, cx| {
                input.text = "file-02".into();
                cx.notify();
            });
        })
        .unwrap();
    settle(&mut cx);
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.active().read(cx).entries.len(), 1);
            assert_eq!(view.tabs.items[0].left.read(cx).entries.len(), 32);
            assert!(view.tabs.items[0].left.read(cx).extension_filter.is_empty());
        })
        .unwrap();
    cx.quit();
}

#[test]
fn compact_tabs_and_view_controls_keep_their_order_in_both_languages() {
    let (_temp, mut cx, window) = setup();
    for language in [
        crate::ui::i18n::Language::English,
        crate::ui::i18n::Language::French,
    ] {
        for width in [900., 720.] {
            window
                .update(&mut cx, |view, _, cx| {
                    view.active().update(cx, |pane, cx| {
                        pane.language = language;
                        cx.notify();
                    });
                })
                .unwrap();
            cx.simulate_window_resize(window.into(), gpui::size(px(width), px(580.)));
            settle(&mut cx);
            let mut visual = gpui::VisualTestContext::from_window(window.into(), &cx);
            assert_eq!(visual.debug_bounds("tabs").unwrap().size.height, px(26.));
            let list = visual.debug_bounds("view-list").unwrap();
            let grid = visual.debug_bounds("view-grid").unwrap();
            let filter = visual.debug_bounds("toggle-filter").unwrap();
            let search = visual.debug_bounds("toggle-search").unwrap();
            assert!(
                list.right() <= grid.left()
                    && grid.right() < filter.left()
                    && filter.right() <= search.left()
            );
            assert_eq!(list.top(), grid.top());
            assert_eq!(grid.top(), filter.top());
            assert_eq!(filter.top(), search.top());
            assert!(visual.debug_bounds("view-columns").is_none());
            visual.simulate_click(grid.center(), Default::default());
            window
                .update(&mut cx, |view, _, cx| {
                    assert_eq!(view.active().read(cx).view_mode, ViewMode::Grid)
                })
                .unwrap();
            visual.simulate_click(list.center(), Default::default());
            window
                .update(&mut cx, |view, _, cx| {
                    assert_eq!(view.active().read(cx).view_mode, ViewMode::List)
                })
                .unwrap();
            visual.simulate_click(filter.center(), Default::default());
            window
                .update(&mut cx, |view, _, cx| {
                    assert!(view.active().read(cx).filter_open)
                })
                .unwrap();
            visual.simulate_click(filter.center(), Default::default());
        }
    }
    cx.quit();
}
