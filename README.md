![Virial logo](assets/images/virial-gpui-banner.png)

# Virial
---

Virial is a Linux file manager for browsing local folders, mounted drives, and recent files. Navigate with the sidebar and breadcrumbs, organize related folders into persistent workspaces, and preview supported images, PDFs, and text files. The browser also supports common file actions, including selecting, copying, moving, renaming, and trashing files.

When a path has more than two breadcrumbs, the toolbar shows an ellipsis followed by the last two. Click a visible breadcrumb to navigate to that folder.

Virial adapts its interface language to the system language. English is used when the system language is not supported.

Context menus show an icon beside each available action.

Documents, Pictures, Music, and Videos in the sidebar use distinct outline icons.

Right-click a file or folder and choose **Copy path** to copy its full path to the clipboard.

Drag files or folders onto a folder in the list, a sidebar location, or a breadcrumb to move them there. The preview closes and the sidebar reappears while dragging. Dragging a selected item moves the entire selection; hold Ctrl to copy instead. Existing destination names are never overwritten, and moves support `Ctrl+Z`.

Drag local files or folders out of Virial onto another application to open, attach, or import them, depending on the receiving application. Dragging a selected item exports the entire selection. External drops offer a copy and keep the originals, on both X11 and Wayland. ZIP members support dragging within Virial only.

USB drives and other removable storage appear automatically under **Devices**, including their filesystem labels, sizes, and mount status. Click a volume to browse it; unmounted volumes are mounted first. The × button unmounts one volume, and the eject button safely removes the entire drive after unmounting all its volumes. Operations fail if a volume is in use. If the current device is disconnected or unmounted, Virial returns to Home. Device management requires the UDisks2 system service (`udisks2` on Debian/Ubuntu); desktop authorization dialogs may appear when needed. Encrypted volume unlocking is handled by your desktop.

Open **Trash** in the sidebar to see deleted files and folders, including items on mounted drives. Select items and click **Restore** in the toolbar or context menu to move them back to their original locations. Existing files are never overwritten; if the original parent folder is missing, recreate it before restoring. Restoration also works after restarting Virial and can be undone with `Ctrl+Z`.

The sidebar footer shows the available space on the filesystem containing your home folder.

Single-click a file or folder to show its preview and details. The preview appears after the double-click interval, so double-clicking opens the item without first showing its preview. The left navigation sidebar gently collapses while a preview is open and returns when the preview closes, freeing space for the file list. Drag the left edge of the preview panel to adjust its width, or use the expand button for a larger view that keeps the window title bar accessible in windowed mode. Virial remembers the preview width and the window size, position, and maximized or fullscreen state when you close it. Window placement under Wayland is controlled by the compositor. Double-click to open the item and hide the details. Navigating to another folder or clicking empty space hides the details; single-click an item to show them again.

PDF previews show the first page, including PDF files inside ZIP archives, and support the expanded view. They require `pdftoppm` from Poppler (`poppler-utils` on Debian/Ubuntu). PDFs up to 20 MiB are rendered locally; invalid, password-protected, or slow files show the unavailable-preview message.

Press `F2` or choose **Rename…** to edit an item's name directly in its row. The file name is selected without its final extension; folder names are selected in full. Press Enter to save, or Escape or click elsewhere to cancel. The extension remains editable. In search and rename fields, use `Ctrl+Backspace` or `Ctrl+Delete` to remove the previous or next word.

Image previews show the image format and pixel dimensions below the image, alongside file size and modification date. Format and dimensions are omitted when the image header cannot be read. This also applies to the expanded view and supported images inside ZIP archives.

Image previews include **Convert image…** and **Remove background…** buttons, also available in the expanded view. Convert PNG, JPEG, WebP, GIF, or BMP images to PNG, JPEG, or WebP and choose an output file name. Animated images export their first frame; JPEG composites transparency onto white. PNG uses high lossless compression and omits unnecessary alpha channels; JPEG uses quality 75 and WebP uses lossy quality 80 with transparency preserved. File size depends on image content and format: converting a JPEG to lossless PNG can still produce a larger file. SVG processing is not supported. Exports create a new file beside the original (including inside ZIP archives), never overwrite an existing file, and support `Ctrl+Z`. Processing runs in the background and accepts images up to 20 MiB and 32 megapixels. Image exports are disabled in Trash.

Press `Ctrl+Z` in the browser to undo the latest file or workspace change. Virial keeps the last 20 changes across restarts, including copy, move, rename, Trash, creation, compression, and ZIP edits. Undo refuses to discard files changed since the action or overwrite conflicting contents. Saved contents are stored under `$XDG_DATA_HOME/virial/undo` (default: `~/.local/share/virial/undo`), so recording large files or folders needs additional disk space. Undoing Trash restores saved contents; the desktop Trash retains its copy. Let an operation finish before closing the window.

Double-click a ZIP archive to browse it like a folder, using breadcrumbs and the usual navigation keys. Preview supported images, PDFs, and text, rename files or entire folders, and use cut/paste or drag-and-drop to move items within a ZIP, between ZIPs, or between a ZIP and a local folder. Ctrl-drag and copy/paste copy items. You can also create files and folders inside a ZIP. Changes are saved directly to the archive; existing destination names are never overwritten. Opening a member in another application uses a temporary copy kept until Virial closes; external edits are not saved back to the ZIP. Nested ZIP browsing and moving members to the desktop Trash are not supported. Archives containing encrypted members, links, or special files cannot be modified; encrypted members and links cannot be extracted.

Click a line in a text or code preview, then use the Up and Down arrow keys to move the highlighted line; the preview scrolls to keep it visible. Code previews use syntax highlighting, a monospace font, and line numbers. Source indentation and blank lines are preserved, with tabs displayed at four-column stops. Use horizontal scrolling or Shift + mouse wheel to read long lines. Supported languages include Rust, Python, JavaScript, TypeScript, C/C++, HTML, CSS, JSON, TOML, and shell scripts; unrecognized text files keep a plain text preview. Text and code previews show up to the first 64 KiB of the file.

Code file icons use official logos for Python, TypeScript, React (`.jsx` and `.tsx`), Rust, HTML, CSS, Sass (`.scss` and `.sass`), and C++, plus community marks for JavaScript and C. Images, music, and common video formats use distinct category icons. Other formats, including JSON, configuration files, shell scripts, PDF, and Markdown, also use distinct category icons. Logos retain their colors in the file list, search results, and details panel. Extension matching is case-insensitive. See [icon sources and notices](assets/icons/README.md).

Use the extension field in the toolbar to filter files in the current folder or recent files. Enter `pdf` or `.pdf`; matching is case-insensitive and uses the final extension (`gz` for `archive.tar.gz`). Folders stay visible for navigation. The filter stays active when navigating; clear the field to show all files again.

Press `Ctrl+P` to open the global path picker. It searches accessible locations as you type, starting with your home folder and visiting nearby folders before deeper trees. Results appear as they are found; partial names such as `jev-codex` match `jev-codex-pilot`, with exact names and name prefixes ranked ahead of path and fuzzy matches. Virial performs this search itself, so it does not require zoxide or a prebuilt search index; arrow keys move through results and Enter opens the selection.

---

## See Virial in action

![Virial logo](assets/images/virial-gpui-interface0.png)
![Virial logo](assets/images/virial-gpui-interface1.png)
![Virial logo](assets/images/virial-gpui-interface2.png)

---

## Quickstart

### Prerequisites

On Debian or Ubuntu, install the native libraries and build tools used by GPUI:

```sh
sudo apt update && sudo apt install -y build-essential pkg-config libfontconfig1-dev libwayland-dev libx11-xcb-dev libxkbcommon-dev libxkbcommon-x11-dev libasound2-dev libvulkan-dev udisks2 poppler-utils
```

Rust stable (1.85 or newer) and Cargo are also required. If they are not installed, install the toolchain with:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Run without installing

From the project directory, build and launch Virial directly:

```sh
cargo run --release
```

This runs the app from the current checkout without installing a permanent copy of the executable.

On first launch, Virial registers its embedded logo and desktop launcher in `$XDG_DATA_HOME` (default: `~/.local/share`), and refreshes KDE's application cache when available. Window icons use the registered desktop launcher and application identity on X11 and Wayland. This also works when running a downloaded executable directly. The launcher follows the executable's location the next time you run it.

To register a downloaded executable before opening the app, run `./virial-gpui --install-desktop`. This needs no graphical session. A raw Linux executable may still have a generic file icon in Downloads; open Virial from the application menu to use its branded launcher.

### Install on this machine

From the project directory, build and install the executable, desktop launcher, and icon under your home directory:

```sh
./install.sh
```

### Tests and performance

GitHub Actions runs the tests, builds the release executable, and checks filesystem performance on Ubuntu for pushes and pull requests. You can also start the workflow manually. Run the same checks locally after installing the prerequisites:

```sh
cargo test --locked
cargo build --release --locked
cargo test --release --locked infrastructure::performance::filesystem_performance -- --ignored --exact --nocapture --test-threads=1
```

The performance smoke test measures listing 5,000 files and searching 5,000 files across 50 folders. It excludes fixture creation, warms the filesystem cache, and reports the median and maximum of seven samples. Each operation must have a median below two seconds; this broad budget catches severe slowdowns on shared runners. Results appear in the workflow summary and the `filesystem-performance` artifact. These checks do not measure graphical rendering, startup, or cold-cache performance.

### Optional background removal

To install Virial with local [rembg](https://github.com/danielgatis/rembg) background removal, use one command from the project directory:

```sh
./install.sh --with-background-removal
```

This requires Python 3.11–3.13 with venv support (`python3-venv` on Debian/Ubuntu) and internet access for dependency installation. Python is used only by rembg; image conversion runs directly in Rust and needs no Python. The installer sets up rembg in `$XDG_DATA_HOME/virial/rembg-venv` (default: `~/.local/share/virial/rembg-venv`). Virial finds it automatically, including from the application menu or `cargo run --release`, with no `PATH` changes. An existing rembg on `PATH` is also supported when no managed installation exists.

Background removal creates a transparent PNG. Both PNG conversion and background removal use additional lossless optimization of compression, palettes, and bit depth to reduce file size while preserving pixels, transparency, and dimensions. Photos can still be larger than JPEG or WebP exports. The first removal downloads the `u2netp` model; subsequent removals work offline with the cached model. Images are processed on your machine. Missing dependencies, model download failures, and invalid images are reported without creating an output file.
