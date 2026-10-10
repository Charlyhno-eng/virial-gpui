#!/bin/sh
set -eu

[ "$#" -eq 0 ] || { echo "Usage: ./build-deb.sh" >&2; exit 2; }
for tool in cargo python3 dpkg-deb dpkg-shlibdeps dpkg-architecture strip; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "Missing $tool. Install Rust, python3, dpkg-dev, and binutils on Debian/Ubuntu." >&2
        exit 1
    }
done

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
metadata=$(cargo metadata --no-deps --locked --format-version 1 --manifest-path "$project_dir/Cargo.toml")
version=$(printf '%s\n' "$metadata" | python3 -c '
import json, sys
package = next(p for p in json.load(sys.stdin)["packages"] if p["name"] == "virial-gpui")
print(package["version"].replace("-", "~", 1))
')
target_dir=$(printf '%s\n' "$metadata" | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')
architecture=$(dpkg-architecture -qDEB_BUILD_ARCH)
output_dir="$target_dir/debian"

# Build natively: shared-library dependencies come from this Debian/Ubuntu host.
[ -z "${CARGO_BUILD_TARGET:-}" ] || {
    echo "Unset CARGO_BUILD_TARGET to build a native Debian package." >&2
    exit 1
}
cargo build --release --locked --manifest-path "$project_dir/Cargo.toml"
mkdir -p "$output_dir"
staging=$(mktemp -d "$output_dir/.package.XXXXXX")
trap 'rm -rf "$staging"' 0
trap 'exit 1' HUP INT TERM
package_dir="$staging/package"
mkdir -p "$package_dir/DEBIAN" "$staging/debian"
chmod 755 "$package_dir" "$package_dir/DEBIAN"

install -Dm755 "$target_dir/release/virial-gpui" "$package_dir/usr/bin/virial-gpui"
strip --strip-unneeded "$package_dir/usr/bin/virial-gpui"
install -Dm644 "$project_dir/packaging/virial-gpui.desktop" "$package_dir/usr/share/applications/virial-gpui.desktop"
install -Dm644 "$project_dir/assets/images/virial-gpui-logo.png" "$package_dir/usr/share/icons/hicolor/512x512/apps/virial-gpui.png"
install -Dm644 "$project_dir/LICENSE" "$package_dir/usr/share/doc/virial-gpui/copyright"
install -Dm644 "$project_dir/assets/icons/README.md" "$package_dir/usr/share/doc/virial-gpui/icons/README.md"
for notice in "$project_dir"/assets/icons/licenses/*.txt; do
    install -Dm644 "$notice" "$package_dir/usr/share/doc/virial-gpui/icons/licenses/$(basename "$notice")"
done

cp "$project_dir/packaging/control" "$staging/debian/control"
shlibdeps_log="$staging/shlibdeps.log"
if ! dependencies=$(cd "$staging" && LC_ALL=C dpkg-shlibdeps -O -e"$package_dir/usr/bin/virial-gpui" 2>"$shlibdeps_log"); then
    cat "$shlibdeps_log" >&2
    exit 1
fi
awk '
    /^dpkg-shlibdeps: warning: diversions involved - output may be incorrect$/ {
        if (getline diversion > 0 && diversion ~ /^ diversion by libc6 (from: \/lib(64)?\/ld-linux-[[:alnum:]_.-]+[.]so[.][0-9]+|to: \/lib(64)?\/ld-linux-[[:alnum:]_.-]+[.]so[.][0-9]+[.]usr-is-merged)$/) {
            next
        }
        print
        if (diversion != "") print diversion
        next
    }
    { print }
' "$shlibdeps_log" >&2
dependencies=${dependencies#shlibs:Depends=}
[ -n "$dependencies" ] || { echo "Cannot determine shared-library dependencies." >&2; exit 1; }
installed_size=$(du -sk "$package_dir/usr" | cut -f1)
cat > "$package_dir/DEBIAN/control" <<EOF
Package: virial-gpui
Version: $version
Architecture: $architecture
Maintainer: Charlyhno <Charlyhno-eng@users.noreply.github.com>
Section: utils
Priority: optional
Homepage: https://github.com/Charlyhno-eng/virial-gpui
Installed-Size: $installed_size
Depends: $dependencies, libfontconfig1, libwayland-client0, libvulkan1, xdg-utils, hicolor-icon-theme
Recommends: libmpv2, poppler-utils, udisks2, mesa-vulkan-drivers
Description: Linux file manager built with GPUI
 Browse and organize local files, mounted drives, and workspaces.
EOF

dpkg-deb --root-owner-group --build "$package_dir" "$output_dir/virial-gpui_${version}_${architecture}.deb"
