//! UI translations. File and directory names are never translated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    French,
}

impl Language {
    pub fn system() -> Self {
        Self::detect(|key| std::env::var(key).ok())
    }

    fn detect(env: impl Fn(&str) -> Option<String>) -> Self {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|key| env(key).filter(|value| !value.is_empty()))
            .unwrap_or_else(|| "C".into());
        let base = locale.split('.').next().unwrap_or("C");
        if base == "C" || base == "POSIX" {
            return Self::English;
        }
        let preferred = env("LANGUAGE")
            .filter(|value| !value.is_empty())
            .unwrap_or(locale);
        for value in preferred.split(':') {
            match value.split(['_', '-', '.', '@']).next().unwrap_or("") {
                "fr" => return Self::French,
                "en" | "C" | "POSIX" => return Self::English,
                _ => {}
            }
        }
        Self::English
    }

    pub fn text(self, english: &'static str) -> &'static str {
        if self == Self::English {
            return english;
        }
        match english {
            "Open with…" => "Ouvrir avec…",
            "Copy" => "Copier",
            "Cut" => "Couper",
            "Paste" => "Coller",
            "Rename…" => "Renommer…",
            "Move to Trash…" => "Déplacer à la corbeille…",
            "Compress (.tar.gz)" => "Compresser (.tar.gz)",
            "New folder…" => "Nouveau dossier…",
            "New file…" => "Nouveau fichier…",
            "New folder" => "Nouveau dossier",
            "New file" => "Nouveau fichier",
            "Copy path" => "Copier le chemin",
            "Properties" => "Propriétés",
            "Path" => "Chemin",
            "Size (bytes)" => "Taille (octets)",
            "Permissions" => "Permissions",
            "Modified" => "Modifié le",
            "Link target" => "Cible du lien",
            "Cancel" => "Annuler",
            "Close" => "Fermer",
            "Confirm" => "Confirmer",
            "Operation failed" => "Échec de l’opération",
            "Invalid file name" => "Nom de fichier invalide",
            "This name is not valid UTF-8" => "Ce nom n’est pas valide en UTF-8",
            "Working…" => "Opération en cours…",
            "No applications found" => "Aucune application trouvée",
            "Edit the full name, including the extension" => {
                "Modifiez le nom complet, extension comprise"
            }
            "This item will be moved to the desktop Trash" => {
                "Cet élément sera déplacé dans la corbeille du bureau"
            }
            "Home" => "Dossier personnel",
            "Desktop" => "Bureau",
            "Documents" => "Documents",
            "Downloads" => "Téléchargements",
            "Pictures" => "Images",
            "Music" => "Musique",
            "Videos" => "Vidéos",
            "File System" => "Système de fichiers",
            "Recent" => "Récents",
            "Network" => "Réseau",
            "PLACES" => "EMPLACEMENTS",
            "DEVICES" => "PÉRIPHÉRIQUES",
            "LOCAL FILES" => "FICHIERS",
            "Back" => "Précédent",
            "Forward" => "Suivant",
            "Up" => "Parent",
            "Open" => "Ouvrir",
            "Refresh" => "Actualiser",
            "Hidden files" => "Fichiers cachés",
            "NAME" => "NOM",
            "KIND" => "TYPE",
            "SIZE" => "TAILLE",
            "Folder" => "Dossier",
            "File" => "Fichier",
            "Image" => "Image",
            "Audio" => "Audio",
            "Archive" => "Archive",
            "Source code" => "Code source",
            "Document" => "Document",
            "Loading…" => "Chargement…",
            "Loading folder…" => "Chargement…",
            "Reading directory contents" => "Lecture du contenu",
            "Folder unavailable" => "Emplacement indisponible",
            "Choose another location or try refreshing" => {
                "Choisissez un autre emplacement ou actualisez"
            }
            "This folder is empty" => "Ce dossier est vide",
            "Hidden files can be shown from the toolbar" => {
                "Affichez les fichiers cachés depuis la barre d’outils"
            }
            "Double-click to open · Enter" => "Double-clic pour ouvrir · Entrée",
            "No recent files" => "Aucun fichier récent",
            "Files opened with Virial and desktop applications appear here" => {
                "Les fichiers ouverts avec Virial et les applications du bureau apparaissent ici"
            }
            "No mounted network locations" => "Aucun emplacement réseau monté",
            "Mount a network share using your desktop, then refresh" => {
                "Montez un partage réseau depuis votre bureau, puis actualisez"
            }
            "Mounted network shares" => "Partages réseau montés",
            "Recently opened files" => "Fichiers ouverts récemment",
            "Cannot read" => "Impossible de lire",
            "Cannot open" => "Impossible d’ouvrir",
            "Cannot save recent history" => "Impossible d’enregistrer l’historique récent",
            "Cannot open Virial" => "Impossible d’ouvrir Virial",
            "File Manager" => "Gestionnaire de fichiers",
            _ => english,
        }
    }

    pub fn size(self, bytes: Option<u64>) -> String {
        let size = crate::files::format_size(bytes);
        if self == Self::French {
            size.replace('.', ",").replace('B', "o")
        } else {
            size
        }
    }

    pub fn counts(self, folders: usize, files: usize) -> String {
        match self {
            Self::French => format!(
                "{folders} dossier{} · {files} fichier{}",
                if folders > 1 { "s" } else { "" },
                if files > 1 { "s" } else { "" }
            ),
            Self::English => format!(
                "{folders} folder{} · {files} file{}",
                if folders == 1 { "" } else { "s" },
                if files == 1 { "" } else { "s" }
            ),
        }
    }
}

#[cfg(test)]
mod tests {
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
}
