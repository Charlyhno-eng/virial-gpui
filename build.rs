//! Windows-only build script: embed the Virial icon and application
//! manifest into the release executable. The icon resource makes the
//! `.exe` show the proper icon in the installer (Explorer copy dialog),
//! on the desktop shortcut, in Alt-Tab and on the taskbar; the
//! manifest enables Common Controls v6 and Per-Monitor V2 DPI awareness
//! so the window does not look blurry on HiDPI displays.
//!
//! `winres` is gated behind the `windows-resources` Cargo feature so the
//! Linux / macOS CI jobs never link it (it requires the MinGW `windres`
//! toolchain). The Windows release job flips the feature on with
//! `--features windows-resources`; downstream installers can either rely
//! on that flag or invoke the build script directly with a custom
//! tooling layer.
#[cfg(all(windows, feature = "windows-resources"))]
fn main() {
    let mut resource = winres::WindowsResource::new();
    resource
        .set_icon_with_id("assets/icons/win/virial-gpui.ico", "1")
        .set_manifest_file("assets/icons/win/virial-gpui.manifest");
    if let Err(error) = resource.compile() {
        eprintln!("virial-gpui: failed to embed Windows resources: {error}");
        std::process::exit(1);
    }
    // Force a rebuild whenever the embedded icon or manifest changes.
    println!("cargo:rerun-if-changed=assets/icons/win/virial-gpui.ico");
    println!("cargo:rerun-if-changed=assets/icons/win/virial-gpui.manifest");
    println!("cargo:rerun-if-changed=assets/icons/win/virial-gpui.rc");
}

#[cfg(not(all(windows, feature = "windows-resources")))]
fn main() {
    // No-op on Linux / macOS / BSD builds, and on Windows when the
    // resource-embedding feature is disabled (e.g. fast `cargo check`
    // loops). The runtime `platform::windows::desktop::register()` call
    // still claims the AppUserModelID at launch time even without
    // embedded resources.
}
