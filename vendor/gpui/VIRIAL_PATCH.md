# Virial's GPUI patch

Based on the crates.io `gpui` 0.2.2 source, licensed under Apache-2.0.
The upstream runtime source and resources are preserved; examples, upstream tests,
and registry metadata are omitted. Cargo uses this copy through `[patch.crates-io]`.

Upstream 0.2.2 supports incoming file drops but has no outgoing native drag API.
Virial adds `Window::start_file_drag` for copy-only `text/uri-list` payloads:

- `platform.rs`, `interactive.rs`, `window.rs`, and `app.rs`: native source API, cancellation,
  completion cleanup, and modifier preservation for internal Wayland drops.
- `platform/linux/wayland/{client,window,clipboard}.rs`: data source ownership,
  URI delivery independent of the clipboard, native drag initiation using the
  current pointer press serial, and preserving typed internal drops on return.
- `platform/linux/x11.rs` and `platform/linux/x11/{client,window,file_drag}.rs`:
  XDND source negotiation, target/proxy discovery, selection requests, release,
  cancellation, and completion timeout.

Virial encodes and validates local paths in `src/platform/linux/file_drag.rs`.
When upgrading GPUI, review these changes against upstream native drag support.
