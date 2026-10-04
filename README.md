# Download

```sh
git clone https://github.com/Charlyhno-eng/virial-gpui.git
cd virial-gpui
```

# Run (Ubuntu/Debian)

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config cmake clang libclang-dev libfontconfig1-dev libfreetype6-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libssl-dev libzstd-dev libvulkan1 mesa-vulkan-drivers xdg-utils libglib2.0-bin tar gzip curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
# Launch the dark file manager (double-click an item or press Enter to open):
cargo run --release
# Right-click files/folders or empty space for context actions.
# Copy/cut/paste within Virial: Ctrl+C / Ctrl+X / Ctrl+V; rename: F2; Trash: Delete.
# Create a folder: Ctrl+Shift+N. Escape closes menus and dialogs.
# Rename edits the full name, including its extension; existing targets are never overwritten.
# Open with lists installed desktop applications; compression creates a sibling .tar.gz archive.
# Optional starting directory:
cargo run --release -- /path/to/directory
# The interface follows LC_ALL / LC_MESSAGES / LANG and LANGUAGE preferences.
# French and English are supported; other languages fall back to English.
# Optional language overrides:
LC_ALL=fr_FR.UTF-8 LANGUAGE=fr cargo run --release
LC_ALL=en_US.UTF-8 LANGUAGE=en cargo run --release
# Recent combines desktop XBEL history and files opened with Virial (up to 200).
# Network lists already mounted GVFS, SMB, NFS, SSHFS, WebDAV and rclone shares.
# Optional GVFS setup and example mount; refresh Network after mounting:
sudo apt-get install -y libglib2.0-bin gvfs-backends gvfs-fuse
gio mount smb://server/share
```
