![Virial logo](assets/images/virial-gpui-banner.png)

# Virial

Virial is a Linux file manager for local folders, mounted drives, and recent files. Browse with the sidebar and breadcrumbs, group folders into persistent workspaces, and preview supported images, PDFs, audio, video, and text. The interface follows the system language when available and otherwise uses English.

## Features

- **File operations:** Select, copy, move, rename, trash, and restore files. Drag items to folders, sidebar locations, or breadcrumbs; dragging a selection moves it, while Ctrl-drag copies. External drops copy local files on X11 and Wayland; ZIP members can only be dragged within Virial. Destinations are never overwritten. Same-filesystem moves use rename; other transfers use accelerated Linux copying where available, with up to four workers on nonrotating drives and one on rotating or unknown drives. Optional SHA-256 verification checks copies before publication; cross-filesystem moves always verify before removing sources. Transfers show phase, item and byte progress, and estimated time. Pause/resume, cancel, or background local transfers and Trash jobs; queued actions can be cancelled. Conflicts compare paths and sizes, identify identical contents, and offer Skip or Keep both (numbered names), once or for all. Moves support Ctrl+Z.
- **Recovery and undo:** Local copies, moves, and Trash jobs are journaled in `$XDG_DATA_HOME/virial/operations`. Interrupted jobs reopen paused; resume checks saved partial contents and skips completed destinations. Changed files stop recovery while journals and backups remain available for retry. I/O errors stop the queue; Retry resumes partial work, and Cancel records completed files for undo. ZIP edits and other queued actions support undo, but not crash recovery or pause. Ctrl+Z keeps the last 20 file and workspace changes across restarts, including copy, move, rename, Trash, creation, compression, and ZIP edits. It refuses to discard changed files or overwrite conflicts. Undo data is stored in `$XDG_DATA_HOME/virial/undo` (default `~/.local/share/virial/undo`) and may require extra disk space; copy-on-write snapshots share blocks when supported. Undoing Trash offers to restore the whole deleted batch and shows its recursive item count; the desktop Trash retains its copy. Each queued job is one undo action. Finish or cancel queued work before undoing, and let active work finish before closing.
- **Devices and Trash:** Removable drives appear under Devices with filesystem labels, sizes, and mount status. Selecting a volume mounts it if needed; × unmounts it, and eject safely removes the drive after unmounting its volumes. Operations fail while a volume is in use; disconnecting the current device returns Virial to Home. Device management needs UDisks2 (`udisks2` on Debian/Ubuntu); desktop authorization may be required, and encrypted volumes are unlocked by the desktop. Trash lists deleted files and folders, including those on mounted drives. Restore from the toolbar or context menu returns items to their original locations without overwriting; recreate a missing parent folder first. Restore works after restart and can be undone.
- **Preview and navigation:** Single-click previews an item after the double-click interval; double-click opens it directly. Navigating or clicking empty space hides details. Press F2 or choose Rename to edit a name in its row: files select the name without the final extension, folders select the full name; Enter saves, Escape or clicking elsewhere cancels, and the extension remains editable. The sidebar collapses while previewing and returns when closed. Resize the preview by dragging its left edge, or expand it while keeping the title bar accessible; Virial remembers preview width and window size, position, and state. Wayland placement is controlled by the compositor. Audio and video play paused in the preview with Play/Pause, ±10 s, Restart, and Mute controls. Closing or changing selection stops playback; hiding pauses it. Formats include MP3, FLAC, OGG, WAV, M4A, MP4, MKV, MOV, AVI, and WebM. Playback requires `libmpv2` (`sudo apt install libmpv2` on Debian/Ubuntu); codecs depend on the installed library. Virial selects an available system audio output. Local files stream at any size; ZIP members are temporarily extracted and limited to 512 MiB. PDFs show their first page, including from ZIPs, and support expanded view. They require Poppler's `pdftoppm` (`poppler-utils` on Debian/Ubuntu); local rendering is limited to 20 MiB. Invalid, protected, or slow files show an unavailable-preview message.
- **Images and text:** Image details include format, dimensions, size, and modification date. Convert PNG, JPEG, WebP, GIF, or BMP to PNG, JPEG, or WebP; animation exports its first frame, JPEG composites transparency on white, and SVG is unsupported. PNG uses high lossless compression, JPEG quality 75, and WebP lossy quality 80 with transparency. Conversion may increase file size (for example, JPEG to PNG). Remove background exports a transparent PNG using local [rembg](https://github.com/danielgatis/rembg); install it with `./install.sh --with-background-removal` or make it available on `PATH`. The first removal downloads the `u2netp` model; later runs can work offline. Exports create a new file beside the original (also inside ZIPs), never overwrite, support Ctrl+Z, and run in the background; input is limited to 20 MiB and 32 megapixels. Exports are disabled in Trash. Code previews have syntax highlighting, line numbers, preserved indentation and blank lines, and four-column tab stops. Scroll horizontally or use Shift+mouse wheel for long lines. Rust, Python, JavaScript, TypeScript, C/C++, HTML, CSS, JSON, TOML, and shell are highlighted; other text is shown plainly, up to 64 KiB. Common file types and popular languages/frameworks have category or logo icons; see [icon sources and notices](assets/icons/README.md).
- **ZIP archives and search:** Double-click ZIPs to browse with breadcrumbs and normal navigation keys. Preview supported content, rename files or folders, create items, and cut, paste, drag, or Ctrl-drag to move or copy within/between ZIPs and local folders. Edits are saved directly; destination names are never overwritten. Opening a member externally uses a temporary copy until Virial closes; external edits are not saved into the ZIP. Nested ZIPs and moving members to desktop Trash are unsupported; archives with encrypted members, links, or special files cannot be modified, and encrypted members and links cannot be extracted. Use the toolbar extension filter in folders or recent files (`pdf` or `.pdf`, case-insensitive final extension); folders remain visible and the filter persists during navigation. Ctrl+P searches accessible locations by name and path, not contents or size. Virial indexes Home first, then other locations in the background and shows initial results progressively. The index is cached at `$XDG_CACHE_HOME/virial/search` (default `~/.cache/virial/search`) and refreshed on launch; folder notifications update local locations, while mounts and other locations reconcile every minute. Search needs no external service; query speed depends on indexed paths, while initial inventory depends on storage speed.

## See Virial in action

![Virial screenshot](assets/images/1.png)
![Virial screenshot](assets/images/2.png)
![Virial screenshot](assets/images/3.png)

## Quickstart

### Prerequisites

On Debian or Ubuntu, install GPUI's native libraries and build tools:

```sh
sudo apt update && sudo apt install -y build-essential pkg-config libfontconfig1-dev libwayland-dev libx11-xcb-dev libxkbcommon-dev libxkbcommon-x11-dev libasound2-dev libvulkan-dev udisks2 poppler-utils libmpv2
```

Install Rust stable (1.85+) and Cargo if needed:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Run or install

From the project directory, run directly with `cargo run --release`, or build and install the executable, launcher, and icon under your home directory with:

```sh
./install.sh
```

On first launch, Virial registers its launcher and icon under `$XDG_DATA_HOME` (default `~/.local/share`). Register a downloaded executable with `./virial-gpui --install-desktop`.

### Tests and performance

GitHub Actions runs tests, a release build, and a filesystem performance check on Ubuntu. Run them locally with:

```sh
cargo test --locked
cargo build --release --locked
cargo test --release --locked infrastructure::performance::filesystem_performance -- --ignored --exact --nocapture --test-threads=1
```

The filesystem smoke test lists and searches 5,000 files across 50 folders, requiring median times below two seconds. Benchmark warm indexed searches over one million synthetic paths (prefix, fuzzy, multi-term, and no-match; two-second smoke budget, excluding inventory and cache loading) with:

```sh
cargo test --release --locked infrastructure::search::index::tests::indexed_search_performance -- --ignored --exact --nocapture --test-threads=1
```

Benchmark copying and moving a 64 MiB file plus 1,000 small files, including progress and undo recording, with:

```sh
cargo test --release --locked infrastructure::performance::transfer_performance -- --ignored --exact --nocapture --test-threads=1
```

This reports warm-cache medians for the current filesystem and has no hardware-dependent limit.
