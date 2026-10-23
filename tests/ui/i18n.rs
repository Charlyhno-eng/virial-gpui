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

#[test]
fn native_language_preferences_are_used_without_environment_overrides() {
    assert_eq!(
        Language::detect_with_fallback(|_| None, || Some("fr-FR:en-US".into())),
        Language::French
    );
    assert_eq!(
        Language::detect_with_fallback(|_| None, || Some("de-DE:fr-CA:en".into())),
        Language::French
    );
    assert_eq!(
        Language::detect_with_fallback(|_| None, || Some("en-US:fr-FR".into())),
        Language::English
    );
    assert_eq!(
        Language::detect_with_fallback(|_| None, || None),
        Language::English
    );
    assert_eq!(
        Language::detect_with_fallback(|_| None, || Some("ja-JP".into())),
        Language::English
    );
    assert_eq!(
        Language::detect_with_fallback(
            |key| (key == "LC_ALL").then(|| "C".into()),
            || panic!("Native preferences must not override an explicit locale")
        ),
        Language::English
    );
    assert_eq!(
        Language::detect_with_fallback(
            |key| (key == "LANG").then(|| "fr_CA.UTF-8".into()),
            || panic!("Native preferences must not override an explicit locale")
        ),
        Language::French
    );
}

#[test]
fn navigation_labels_are_translated_and_unknown_languages_fall_back_to_english() {
    for (english, french) in [
        ("List", "Liste"),
        ("Grid", "Grille"),
        ("New tab", "Nouvel onglet"),
        ("Close tab", "Fermer l’onglet"),
        ("Two panes", "Deux panneaux"),
        ("Filter files", "Filtrer les fichiers"),
    ] {
        assert_eq!(Language::English.text(english), english);
        assert_eq!(Language::French.text(english), french);
        assert_eq!(detect(&[("LANG", "ja_JP")]).text(english), english);
    }
}
