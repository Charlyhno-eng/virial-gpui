#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
bin_dir="$HOME/.local/bin"

cargo build --release --manifest-path "$project_dir/Cargo.toml"

install -Dm755 "$project_dir/target/release/virial-gpui" "$bin_dir/virial-gpui"
"$bin_dir/virial-gpui" --install-desktop
