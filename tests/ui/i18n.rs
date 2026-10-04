use super::*;
fn detect(values: &[(&str, &str)]) -> Language {
    Language::detect(|key| {
        values
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    })
}
#[test]
fn honors_locale_precedence_and_language_preferences() {
    assert_eq!(detect(&[("LANG", "fr_FR.UTF-8")]), Language::French);
    assert_eq!(
        detect(&[("LC_ALL", "en_US.UTF-8"), ("LANG", "fr_FR")]),
        Language::English
    );
    assert_eq!(
        detect(&[("LC_MESSAGES", "fr_CA"), ("LANG", "en_US")]),
        Language::French
    );
    assert_eq!(
        detect(&[("LANG", "de_DE"), ("LANGUAGE", "de:fr:en")]),
        Language::French
    );
    assert_eq!(
        detect(&[("LC_ALL", "C.UTF-8"), ("LANGUAGE", "fr")]),
        Language::English
    );
    assert_eq!(detect(&[("LANG", "ja_JP")]), Language::English);
}
