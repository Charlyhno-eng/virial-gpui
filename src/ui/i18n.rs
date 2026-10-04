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
            "Display" => "Afficher",
            "Compact list" => "Liste compacte",
            "Folder information" => "Informations du dossier",
            "Search in" => "Rechercher dans",
            "Type" => "Type",
            "Size" => "Taille",
            "Contents" => "Contenu",
            "Location" => "Emplacement",
            "DETAILS" => "DÉTAILS",
            "Selected item" => "Élément sélectionné",
            "Current location" => "Emplacement actuel",
            "Preview" => "Aperçu",
            "Preview · Space" => "Aperçu · Espace",
            "Loading preview…" => "Chargement de l’aperçu…",
            "No content preview available" => "Aucun aperçu du contenu disponible",
            "Workspaces" => "Espaces de travail",
            "New workspace…" => "Nouvel espace de travail…",
            "Workspace name" => "Nom de l’espace de travail",
            "Enter a workspace name" => "Saisissez un nom d’espace de travail",
            "Add folder to workspace" => "Ajouter le dossier à un espace de travail",
            "Use an existing name to add this folder, or a new name to create a workspace" => {
                "Utilisez un nom existant pour ajouter ce dossier, ou un nouveau nom pour créer un espace de travail"
            }
            "Create an empty workspace, then add folders from the toolbar" => {
                "Créez un espace de travail vide, puis ajoutez des dossiers depuis la barre d’outils"
            }
            "Logical groups of folders · Ctrl+W / Escape returns" => {
                "Groupes logiques de dossiers · Ctrl+W / Échap pour revenir"
            }
            "No workspaces yet" => "Aucun espace de travail",
            "Browse a folder and use Add folder to workspace in the toolbar" => {
                "Ouvrez un dossier puis utilisez Ajouter le dossier à un espace de travail dans la barre d’outils"
            }
            "Remove workspace" => "Supprimer l’espace de travail",
            "Remove association" => "Retirer l’association",
            "Counts and activity cover immediate folder contents" => {
                "Les compteurs et l’activité concernent le contenu direct des dossiers"
            }
            "Recently modified files" => "Fichiers récemment modifiés",
            "Cannot read workspaces" => "Impossible de lire les espaces de travail",
            "Cannot save workspace" => "Impossible d’enregistrer l’espace de travail",
            "Search everywhere" => "Rechercher partout",
            "Search everywhere · Ctrl+P" => "Rechercher partout · Ctrl+P",
            "Type a name or path; spaces separate search terms" => {
                "Saisissez un nom ou un chemin ; séparez les termes par des espaces"
            }
            "Searching…" => "Recherche…",
            "No matches found" => "Aucun résultat",
            "Best 100 matches · ↑/↓ select · Enter opens · Escape closes" => {
                "100 meilleurs résultats · ↑/↓ sélectionner · Entrée ouvrir · Échap fermer"
            }
            "Unreadable locations skipped" => "Emplacements illisibles ignorés",
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
            "Minimize" => "Réduire",
            "Maximize" => "Agrandir",
            "Restore" => "Restaurer",
            "Full screen · F11" => "Plein écran · F11",
            "Exit full screen · F11" => "Quitter le plein écran · F11",
            "Close" => "Fermer",
            "Confirm" => "Confirmer",
            "Operation failed" => "Échec de l’opération",
            "Invalid file name" => "Nom de fichier invalide",
            "This name is not valid UTF-8" => "Ce nom n’est pas valide en UTF-8",
            "Drop here" => "Déposer ici",
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
            "Selected items will be moved to the desktop Trash" => {
                "Les éléments sélectionnés seront déplacés vers la corbeille du bureau"
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
        let size = crate::domain::models::format_size(bytes);
        if self == Self::French {
            size.replace('.', ",").replace('B', "o")
        } else {
            size
        }
    }

    pub fn item_count(self, count: usize) -> String {
        match self {
            Self::French => format!("{count} élément{}", if count > 1 { "s" } else { "" }),
            Self::English => format!("{count} item{}", if count == 1 { "" } else { "s" }),
        }
    }

    pub fn selected_count(self, count: usize) -> String {
        match self {
            Self::French => format!(
                "{count} élément{} sélectionné{}",
                if count > 1 { "s" } else { "" },
                if count > 1 { "s" } else { "" }
            ),
            Self::English => format!("{count} item{} selected", if count == 1 { "" } else { "s" }),
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
#[path = "../../tests/ui/i18n.rs"]
mod tests;
