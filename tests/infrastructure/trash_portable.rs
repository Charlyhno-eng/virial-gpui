use super::trash;
use std::path::PathBuf;

#[cfg(not(unix))]
#[test]
fn portable_trash_stub_reports_unsupported_instead_of_panicking() {
    let data = PathBuf::from(std::env::temp_dir().join("virial-trash-stub"));
    let error = trash::read(&data).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
    let error = trash::restore(&data, &[data.clone()]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
}