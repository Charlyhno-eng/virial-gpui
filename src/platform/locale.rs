//! Native language preferences for desktop launches without locale variables.

#[cfg(windows)]
pub(crate) fn preferred_languages() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut count = 0;
    let mut length = 0;
    // Windows first reports the required UTF-16 buffer size, including NULs.
    unsafe {
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut length,
        ) == 0
            || length == 0
        {
            return None;
        }
        let mut buffer = vec![0u16; length as usize];
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            buffer.as_mut_ptr(),
            &mut length,
        ) == 0
        {
            return None;
        }
        decode_windows_languages(&buffer[..length as usize])
    }
}

#[cfg(any(windows, test))]
fn decode_windows_languages(buffer: &[u16]) -> Option<String> {
    let languages = String::from_utf16(buffer).ok()?;
    let languages = languages
        .split('\0')
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(":");
    (!languages.is_empty()).then_some(languages)
}

#[cfg(target_os = "macos")]
pub(crate) fn preferred_languages() -> Option<String> {
    use core_foundation_sys::{
        array::{CFArrayGetCount, CFArrayGetValueAtIndex},
        base::CFRelease,
        locale::CFLocaleCopyPreferredLanguages,
        string::{CFStringGetCString, kCFStringEncodingUTF8},
    };
    // The copied array owns its strings until CFRelease; no ObjC runtime or
    // shell command is needed, including when started directly from Finder.
    unsafe {
        let array = CFLocaleCopyPreferredLanguages();
        if array.is_null() {
            return None;
        }
        let mut languages = Vec::new();
        for index in 0..CFArrayGetCount(array) {
            let value = CFArrayGetValueAtIndex(array, index);
            let mut buffer = [0i8; 256];
            if !value.is_null()
                && CFStringGetCString(
                    value.cast(),
                    buffer.as_mut_ptr(),
                    buffer.len() as isize,
                    kCFStringEncodingUTF8,
                ) != 0
            {
                languages.push(
                    std::ffi::CStr::from_ptr(buffer.as_ptr())
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        CFRelease(array.cast());
        let languages = languages.join(":");
        (!languages.is_empty()).then_some(languages)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn preferred_languages() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_preferences_preserve_order_and_ignore_terminators() {
        let buffer = "de-DE\0fr-FR\0en-US\0\0".encode_utf16().collect::<Vec<_>>();
        assert_eq!(
            decode_windows_languages(&buffer).as_deref(),
            Some("de-DE:fr-FR:en-US")
        );
        assert_eq!(decode_windows_languages(&[0, 0]), None);
        assert_eq!(decode_windows_languages(&[0xD800]), None);
    }
}

#[cfg(all(test, any(windows, target_os = "macos")))]
#[test]
fn native_desktop_preferences_are_readable() {
    let languages =
        preferred_languages().expect("The desktop must expose its preferred UI languages");
    assert!(!languages.is_empty());
    assert!(!languages.contains('\0'));
}
