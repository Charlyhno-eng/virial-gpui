//! Portable fallback for desktop identity. Registration is a no-op: the
//! portable target has no desktop-entry database to update.
pub(crate) const APP_ID: &str = "virial-gpui";

pub(crate) fn register() -> std::io::Result<()> {
    Ok(())
}
