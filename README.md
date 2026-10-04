# Download

```sh
git clone https://github.com/Charlyhno-eng/virial-gpui.git
cd virial-gpui
```

# Run (Ubuntu/Debian)

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config cmake clang libclang-dev libfontconfig1-dev libfreetype6-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libssl-dev libzstd-dev libvulkan1 mesa-vulkan-drivers xdg-utils curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
cargo run --release
# Optional starting directory:
cargo run --release -- /path/to/directory
```
