#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
bin_dir="$HOME/.local/bin"

case "${1:-}" in
    "")
        [ "$#" -eq 0 ] || { echo "Usage: ./install.sh [--with-background-removal]" >&2; exit 2; }
        ;;
    --with-background-removal)
        [ "$#" -eq 1 ] || { echo "Usage: ./install.sh [--with-background-removal]" >&2; exit 2; }
        python_bin=""
        for candidate in python3 python3.13 python3.12 python3.11; do
            if command -v "$candidate" >/dev/null 2>&1 &&
                "$candidate" -c 'import sys; sys.exit(not ((3, 11) <= sys.version_info[:2] < (3, 14)))'; then
                python_bin="$candidate"
                break
            fi
        done
        if [ -z "$python_bin" ]; then
            echo "Background removal requires Python 3.11–3.13 with venv support (python3-venv on Debian/Ubuntu)." >&2
            exit 1
        fi
        case "${XDG_DATA_HOME:-}" in
            /*) data_dir="$XDG_DATA_HOME" ;;
            *) data_dir="$HOME/.local/share" ;;
        esac
        rembg_env="$data_dir/virial/rembg-venv"
        "$python_bin" -m venv "$rembg_env" || {
            echo "Install venv support for $python_bin (python3-venv on Debian/Ubuntu), then retry." >&2
            exit 1
        }
        "$rembg_env/bin/python" -m pip install 'rembg[cpu,cli]'
        ;;
    *) echo "Usage: ./install.sh [--with-background-removal]" >&2; exit 2 ;;
esac

cargo build --release --manifest-path "$project_dir/Cargo.toml"

install -Dm755 "$project_dir/target/release/virial-gpui" "$bin_dir/virial-gpui"
"$bin_dir/virial-gpui" --install-desktop
