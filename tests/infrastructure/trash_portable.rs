#[cfg(not(unix))]
use super::trash;

#[cfg(not(unix))]
#[test]
fn portable_trash_stub_reports_unsupported_instead_of_panicking() {
    let data = std::env::temp_dir().join("virial-trash-stub");
    let error = trash::read(&data).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
    let error = trash::restore(&data, std::slice::from_ref(&data)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
}
