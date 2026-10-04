# Virial

Virial is a Linux file manager built with Rust and GPUI.

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
