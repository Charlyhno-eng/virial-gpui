# Virial

Virial is a Linux file manager built with Rust and GPUI.

The interface uses compact spacing and typography, with a subdued dark theme,
muted blue-gray accents, and a subtle violet tint in the background, inspired by
the application logo. The initial window size adapts to smaller displays. The
dark background is slightly more opaque while remaining transparent when
supported by the desktop compositor; text, menus, and dialogs remain readable.
Folder navigation gently recedes and settles over approximately 150 ms. Selection
uses a faint blue-gray light, each breadcrumb responds to hover, and drag previews
show the file name and item count with a softly appearing “Drop here” hint on the
destination. Short fades accompany opening menus or dialogs, with a
subtle activity indicator while work is in progress.

Use the compact toolbar search field to find files and folders anywhere,
independently of your current location. Its placeholder indicates the global scope;
**Ctrl+P** focuses the same field. Type a name or path: space-separated terms match
case-insensitively, with exact names ranked before partial and fuzzy (letters in
order) path matches. The best 100 results appear directly below the toolbar with
their full paths while the background search progresses. **Enter** opens the
highlighted folder in Virial or file in its default application; **Up/Down** selects
another result, and clicking a result also opens it. If Enter precedes the first
results, opening waits for an exact name or the completed scan. **Escape**, **Close**,
or clearing the field cancels the search. There is no separate search dialog or
“Search everywhere” button, and typing no longer filters the current folder.

Global search scans the home folder first, then the rest of the accessible
filesystem, including mounted disks. It requires no zoxide installation or
external index and finds items you have never visited. The **Ctrl+H** hidden-file
setting at the time the query starts also applies to global search. Unreadable
locations are skipped and counted; `/proc`, `/sys`, and `/dev` are excluded.
Directory symlinks appear as results but are not traversed, preventing cycles.
Changing the query or closing the results cancels the pending search. Large disks
or slow mounts can take time to search; results remain usable during the scan.

Press **Ctrl+W** or select **Workspaces** in the sidebar to open logical groups
of folders. While browsing a folder, choose **Display → Add folder to workspace**, then select an existing workspace or enter a new name.
Repeat from other folders to group them together. **New workspace…** creates an
empty group. Workspace cards open their associated folders and show file/folder
counts plus the five most recently modified files, which you can click to open.
Counts and activity cover the immediate contents of associated folders, respect
**Ctrl+H**, and refresh with **F5**; they do not scan subfolders or record change
events. Unavailable folders are flagged while the other folders remain usable.
**Ctrl+W** or **Escape** returns through navigation history.

Workspace associations persist in `$XDG_DATA_HOME/virial/workspaces` (by default
`~/.local/share/virial/workspaces`). Adding the same folder again does not duplicate
it. **Remove association** and **Remove workspace** only remove the logical group
configuration; your files and folders stay in place.

By default, the browser shows only the sidebar and file list. Selecting an item
opens a temporary preview beside the files. Press **Space** to expand it across
the application window (press Space again to return to the side panel). **Escape**
or **Close** dismisses the preview and clears the selection, restoring the two-panel
layout. Selecting another item opens its preview automatically. The **Preview**
option in **Display** can hide or reopen the panel for the current selection.
Enable **Display → Folder information** to keep a right-hand information panel
for the current folder, including while selecting items. This mode replaces the
temporary side preview; **Space** still opens the selected item’s expanded preview.
It shows the folder path, type, item count, and modification time (UTC). Selected folders also show their immediate
item count; counts respect the hidden-file setting.

Previews show images up to 20 MiB or the first 64 KiB of UTF-8 text, loaded in the
background. Folders, binary files, and unsupported formats show item information
instead. **Display** groups compact-list and hidden-file settings with adding the current folder to a workspace. Click the **Name** column
heading to switch between ascending and descending alphabetical order; folders
stay first and selections remain attached to their files. Navigation buttons,
interactive breadcrumbs, global search, refresh, and Display share the toolbar.
The sidebar, breadcrumbs, and navigation buttons remain available while browsing.

The file list appears without waiting for recursive folder sizes. Its size column
shows file sizes immediately and fills in folder totals in background batches;
folders show `—` until their size is available. Leaving or refreshing a location
cancels its pending size calculations. Refresh with **F5** to recalculate sizes
after external changes. Folder totals include hidden files and skip nested
symbolic links to avoid cycles and duplicate counts.

Use the title bar's full-screen control or **F11** to fill the entire display,
including the space normally reserved for desktop panels. **F11**, **Escape**, or
the toolbar's exit control returns to the previous window state. Escape closes an
open menu or dialog first. The maximize control and a double-click on the title
bar expand the window within the desktop's work area.

Select files and folders with a click, **Ctrl-click** to toggle individual items,
or **Shift-click** to select a range. **Ctrl+Shift-click** adds a range to the
selection, and **Ctrl+A** selects all items. Drag from the empty area below the
rows or the narrow left gutter to draw a selection rectangle; hold **Ctrl** or
**Shift** to add to the selection. Dragging near the top or bottom scrolls the
list. Click empty space or press **Escape** to clear the selection.

Navigate without the mouse while keeping the toolbar buttons available:
**Up/Down** select entries, **Home/End** jump to the first/last entry, and
**Page Up/Page Down** move by one visible page. Hold **Shift** with these keys
to extend the selection. **Enter** opens the selected entry; **Right** opens a
selected folder, and **Left** or **Backspace** goes to its parent.
**Alt+Left/Alt+Right** navigate back/forward through history, **Alt+Up** goes
to the parent folder, and **Ctrl+H** toggles hidden files.

Drag a selected file or folder to move the whole selection into a folder row,
a directory in the sidebar, or a breadcrumb. Hold **Ctrl** while dropping to
copy instead; **Escape** cancels an active drag. Dropping on the current folder's background also accepts files;
files dropped from another application are copied, preserving their originals.
Copy, cut, paste, copy path, and Move to Trash apply to the whole selection.
Rename, Open with, compression, and properties require one selected item.

Transfers run in the background and never overwrite existing items. Conflicting
names and destinations inside a selected folder are rejected before transferring
any items. Dropping items into their existing parent is a no-op when moving.
If an I/O error interrupts a batch, already completed transfers remain in place;
the list refreshes and displays the error.

## Prerequisites

- Linux with a graphical desktop session
- Rust stable and Cargo, installed with rustup
- On Ubuntu or Debian: `build-essential`, `pkg-config`, `cmake`, `clang`, `libclang-dev`, `libfontconfig1-dev`, `libfreetype6-dev`, `libxcb1-dev`, `libxkbcommon-dev`, `libxkbcommon-x11-dev`, `libwayland-dev`, `libssl-dev`, `libzstd-dev`, `libvulkan1`, `mesa-vulkan-drivers`, `xdg-utils`, `libglib2.0-bin`, `tar`, `gzip`, and `curl`

## Run from source

From the project directory, launch the application with:

```sh
cargo run --release
```

## Install on your PC

From the project directory, build and install Virial with:

```sh
./install.sh
```

The installer adds the application to `~/.local/bin`, creates a desktop menu entry, and installs the Virial logo as its application icon. You can then launch Virial from your desktop's application menu.

## Project structure

```text
assets/
  icons/                  SVG interface icons
  images/                 Application logo
src/
  main.rs                 GPUI startup and window creation
  app.rs                  Application orchestration
  config.rs               Startup path configuration
  ui/
    shell.rs              Main layout and rendering
    theme.rs              Colors and spacing
    icons.rs, i18n.rs      Embedded assets and translations
    components/           Buttons, input, sidebar, modals, title bar, toolbar
    screens/              File browser and workspace screens
  domain/                 File models, locations, navigation services
  infrastructure/         Directory storage, workspaces, recent history, file operations
  state/                  Application state and user actions
  platform/linux/         Desktop applications, places, network mounts
tests/                    Unit test sources grouped by layer
```

The structure follows the application's current features. Additional screens,
platforms, fonts, and persistence backends can be added when needed. Tests live
in `tests/` and are included as unit test modules so they can verify internal
helpers without exposing them as a public library. Run them with `cargo test`.
