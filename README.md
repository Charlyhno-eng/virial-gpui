# Virial

Virial is a Linux file manager built with Rust and GPUI.

The interface uses compact spacing and typography, with a subdued dark theme,
muted blue-gray accents, and a subtle violet tint in the background, inspired by
the application logo. The initial window size adapts to smaller displays. The
dark background is slightly more opaque while remaining transparent when
supported by the desktop compositor; text, menus, and dialogs remain readable.
Short fades accompany folder navigation and opening menus or dialogs, with a
subtle activity indicator while work is in progress.

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
    screens/home.rs       File browser screen
  domain/                 File models, locations, navigation services
  infrastructure/         Directory storage, recent history, file operations
  state/                  Application state and user actions
  platform/linux/         Desktop applications, places, network mounts
tests/                    Unit test sources grouped by layer
```

The structure follows the application's current features. Additional screens,
platforms, fonts, and persistence backends can be added when needed. Tests live
in `tests/` and are included as unit test modules so they can verify internal
helpers without exposing them as a public library. Run them with `cargo test`.
