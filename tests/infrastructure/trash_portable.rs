#[cfg(windows)]
use super::trash;

#[cfg(windows)]
#[test]
fn the_recycle_bin_reports_a_missing_item_instead_of_panicking() {
    let data = std::env::temp_dir().join("virial-trash-item");
    // A path with no $I twin is not a Recycle Bin item: restore refuses
    // with a plain error instead of dereferencing the metadata blindly.
    let error = trash::restore(&data, std::slice::from_ref(&data)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    // Listing every volume's bin is best-effort and must never panic.
    assert!(trash::read(&data).is_ok());
}
