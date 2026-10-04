#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
bin_dir="$HOME/.local/bin"
data_dir=${XDG_DATA_HOME:-"$HOME/.local/share"}
icon_dir="$data_dir/icons/hicolor/512x512/apps"
applications_dir="$data_dir/applications"

cargo build --release --manifest-path "$project_dir/Cargo.toml"

install -Dm755 "$project_dir/target/release/virial-gpui" "$bin_dir/virial-gpui"
install -Dm644 "$project_dir/assets/images/virial-gpui-logo.png" "$icon_dir/virial-gpui.png"
mkdir -p "$applications_dir"

desktop_exec=$(printf '%s' "$bin_dir/virial-gpui" | sed 's/[\\"]/\\&/g')
cat > "$applications_dir/virial-gpui.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Virial
Comment=Linux file manager
Exec="$desktop_exec"
Icon=virial-gpui
Terminal=false
Categories=System;FileTools;FileManager;
EOF
