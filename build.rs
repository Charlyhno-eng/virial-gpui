//! Windows-only build script: embed the Virial icon into the release
//! executable. The icon resource makes the `.exe` show the proper icon
//! in the installer (Explorer copy dialog), on the desktop shortcut,
//! in Alt-Tab and on the taskbar; the per-monitor DPI awareness is
//! applied at runtime by `SetProcessDpiAwarenessContext` in
//! `platform::windows::desktop`, because MSVC link.exe also injects a
//! default manifest of its own and shipping one through winres would
//! double the resource (CVTRES CVT1100).
//!
//! `winres` is gated behind the `windows-resources` Cargo feature so
//! the Linux / macOS CI jobs never link it (it requires the MinGW
//! `windres` toolchain). The Windows release job flips the feature on
//! with `--features windows-resources`; downstream installers can
//! either rely on that flag or invoke the build script directly with
//! a custom tooling layer.
#[cfg(all(windows, feature = "windows-resources"))]
fn main() {
    let mut resource = winres::WindowsResource::new();
    resource.set_icon_with_id("assets/icons/win/virial-gpui.ico", "1");
    // Without a version resource, Explorer's details pane and the taskbar
    // fall back to the crate name and call the file `virial-gpui`. These are
    // what a user reads to identify the program, so they carry the product name.
    resource.set("FileDescription", "Virial");
    resource.set("ProductName", "Virial");
    resource.set("OriginalFilename", "virial-gpui.exe");
    resource.set("InternalName", "virial-gpui");
    resource.set("CompanyName", "Virial");
    resource.set("LegalCopyright", "Distributed under the MIT licence.");
    if let Err(error) = resource.compile() {
        eprintln!("virial-gpui: failed to embed Windows resources: {error}");
        std::process::exit(1);
    }
    // Force a rebuild whenever the embedded icon changes.
    println!("cargo:rerun-if-changed=assets/icons/win/virial-gpui.ico");
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
