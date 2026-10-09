# Changelog

Tous les changements notables de Virial sont documentés dans ce fichier.

La format est basé sur [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
et ce projet adhère à [Semantic Versioning](https://semver.org/lang/fr/).

> **Convention du dépôt** : toute PR qui touche le comportement utilisateur, la
> CI ou le packaging **doit** ajouter une entrée ici sous la section
> « Non publié » (voir AGENTS.md).

## [Non publié]

### Ajouts
- **Qualité obligatoire en CI** : nouveau job `lint` sur chaque PR —
  `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D
  warnings` (tests compris, zéro warning toléré) et vérification qu'une
  entrée CHANGELOG « Non publié » est bien présente.
- **Procédure de publication** : `RELEASE.md` décrit le chemin complet
  (version → CHANGELOG → fmt → clippy → tests → tag `v*` → GitHub Release),
  et le workflow de release attache désormais `virial-gpui-windows-x64.exe`
  à la GitHub Release en plus du paquet Debian.
- **Navigation SSH de bout en bout** : dialog de connexion complet avec
  sélection de la méthode d'authentification (agent / fichier de clé / mot de
  passe), agent SSH Windows (Pageant puis pipe OpenSSH) avec repli automatique
  sur les clés par défaut `~/.ssh/id_ed25519` / `id_rsa`, navigation réelle
  dans les dossiers distants (double-clic), chemins SFTP normalisés en `/`
  (fini les `/home\utilisateur` sous Windows), timeout de connexion à 10 s et
  erreurs de connexion désormais affichées dans le bandeau global.
- **Découverte des volumes sous Windows** : la section DEVICES liste C:\, les
  autres disques, lecteurs optiques et clés USB (label, taille, point de
  montage) ; le moniteur rafraîchit par polling toutes les 2 s. L'éjection
  propre reste signalée comme non supportée.
- **Paquet Windows** : `cargo build --release` produit `dist/virial-gpui.exe`
  (cible `dist/`, ignorée par git) ; la CI publie l'exe en artefact sur chaque
  push (job `windows`).
- `CHANGELOG.md` devient obligatoire pour toute PR (règle AGENTS.md).

### Corrections
- **Windows — le tri de fichiers** : renommer ou déplacer un dossier échouait
  systématiquement (« Accès refusé ») car l'ouverture du handle de snapshot
  undo n'utilisait pas `FILE_FLAG_BACKUP_SEMANTICS`.
- **Windows — Supprimer** : la suppression passait par `gio` (binaire Linux
  absent) et échouait en boucle ; elle emprunte maintenant le service de
  corbeille de la plateforme (Corbeille Windows via PowerShell).
- **Windows — Copier une arborescence** : fausse erreur « Folder tree contains
  a cycle » (identité de dossier `(taille, 0)` — tous les dossiers collident) ;
  l'identité est désormais le chemin résolu.
- **Recherche « partout »** : sous Windows l'index n'explorait qu'une fraction
  arbitraire du profil (même identité) et partait d'une racine `/` sans
  signification ; il indexe maintenant le profil utilisateur avec exclusion
  d'`AppData`, `node_modules`, `$Recycle.Bin`, etc. Linux inchangé.
- **Home portable** : lancé par double-clic (sans `HOME`), l'app écrivait ses
  journaux, favoris SSH et historique à la racine du disque et amputait la
  barre latérale ; la chaîne HOME → USERPROFILE → HOMEDRIVE+HOMEPATH est
  centralisée dans `config::home()`.
- **Fichiers en lecture seule** : la purge des sauvegardes undo/staging retire
  l'attribut avant suppression (Windows), débloquant Ctrl+Z et le nettoyage.
- **Verrous inter-processus sous Windows** : `LockFileEx` remplace le no-op ;
  deux fenêtres ne peuvent plus corrompre les journaux (message clair).
- Linux : l'ouverture des handles de timestamps reste en lecture seule
  (régression `EISDIR`/`EACCES` du portage corrigée en amont de cette branche,
  commit `5500acc`).

### Interface
- L'indicateur remote `><` siège en tête de la ligne de pied de sidebar,
  séparé du libellé « LOCAL FILES » par un filet vertical ; la barre de statut
  séparée disparaît et la liste de fichiers descend jusqu'au bord.
