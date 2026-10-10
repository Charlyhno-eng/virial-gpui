![Virial logo](assets/images/virial-gpui-banner.png)

# Virial

Virial is a file manager for local and remote files on Linux, macOS and Windows. Browse folders, mounted drives, recent files, ZIP archives, and SSH/SFTP hosts in one place. Persistent workspaces, fast search, rich previews, and recoverable file operations make everyday file management easier and safer.

## Features

- **Browse anywhere:** Navigate local folders, recent files, persistent workspaces, mounted drives, ZIP archives, and remote servers over SSH/SFTP. Save host favorites and connect with your SSH agent, a private key, or a password; credentials are not saved.
- **Manage files safely:** Copy, move, rename, trash, and restore items with drag and drop, progress controls, and conflict handling. Destinations are never overwritten. Undo spans file and workspace changes, while local transfers and Trash jobs can recover after interruption.
- **Find files quickly:** Search by name or path across accessible locations, or filter a folder by file extension. A local index provides progressive results and keeps itself up to date.
- **Preview and organize:** Preview images, PDFs, audio, video, and text or code files. Edit ZIP archives, convert common image formats, remove image backgrounds, and group folders into workspaces.
- **Use Linux devices:** Browse and manage removable drives, including mount, unmount, and safe eject through UDisks2. The interface follows the system language when available and otherwise uses English.

## Install

Every release ships one archive per platform. Download the one that matches your
system, then follow the platform notes below.

| Platform | Asset | Notes |
| --- | --- | --- |
| Linux x86_64 | `virial-gpui-<version>-linux-x86_64.tar.gz` | Standalone: binary, desktop entry and icon |
| Linux x86_64 (Debian/Ubuntu) | `virial-gpui_<version>_amd64.deb` | Installs to `/usr/bin`, adds a launcher |
| macOS | `Virial-<version>-macos-<arch>.app.tar.gz` | Untar, then drag `Virial.app` to Applications |
| Windows x86_64 | `virial-gpui-<version>-windows-x86_64.zip` | No console window; `virial-gpui.exe` runs on its own |

On Windows, `virial-gpui.exe --install-desktop` registers the launcher, the taskbar
identity and the `.env` / `.gitignore` / `.toml` file associations. Pass
`--uninstall-desktop` to remove them. Virial registers itself under *Open with*
without replacing an editor you already assigned.

Each release also carries a `SHA256SUMS` file:

```sh
sha256sum --check SHA256SUMS
```

The macOS bundle is ad-hoc signed, so Gatekeeper shows a warning on the first
launch until macOS records the exception (right-click, *Open*).

## See Virial in action

![Virial screenshot](assets/images/virial-gpui-demo.gif)

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

Standalone binaries register their launcher and icon on first launch under `$XDG_DATA_HOME` (default `~/.local/share`). Register a downloaded executable with `./virial-gpui --install-desktop`.

Build a `.deb` (Debian/Ubuntu; requires `python3`, `dpkg-dev`, `binutils`): `./build-deb.sh`. Output: `target/debian`. Matching `v<version>` tags publish Ubuntu 24.04 amd64 packages to [GitHub Releases](https://github.com/Charlyhno-eng/virial-gpui/releases).

### Tests

GitHub Actions runs the test suite, a release build, and a filesystem performance check on Ubuntu. Run the tests locally with `cargo test --locked`.
