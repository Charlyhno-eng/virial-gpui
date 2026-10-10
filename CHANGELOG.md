# Changelog

Tous les changements notables de Virial sont documentés dans ce fichier.

La format est basé sur [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
et ce projet adhère à [Semantic Versioning](https://semver.org/lang/fr/).

> **Convention du dépôt** : toute PR qui touche le comportement utilisateur, la
> CI ou le packaging **doit** ajouter une entrée ici sous la section
> « Non publié » (voir AGENTS.md).

## [Non publié]

- **CI plus rapide** : le cache `actions/cache` couvre le registre cargo et les
  artefacts de dépendances (`.rlib`/`.rmeta`) au lieu de reconstruire ~150
  crates de gpui à chaque run, et `CARGO_INCREMENTAL=0` évite d'embarquer 6 Go
  d'artefacts incrémentaux qui ne servent à rien sur un runner jetable. La
  construction du paquet Debian et les deux benchmarks `#[ignore]` sortent du job
  Linux pour aller dans un job `package-and-benchmarks` séparé, exécuté **en
  parallèle** des trois jobs de test : le job Linux ne fait plus que compiler et
  tester. Le `.deb` n'est produit que sur `push`, sur `main` ou quand la PR
  porte le label `packaging`, ce qui évite de payer sa compilation quand la
  modification ne peut pas l'affecter. Le job Windows construit et dépose
  désormais l'exe release, ce qui donne un binaire à tester sans attendre une
  release. Le garde-fou de changelog est passé du job `merge-readiness` (qui
  tournait en double) au job `lint`, et les runs s'interromment automatiquement
  quand une nouvelle poussée arrive sur la même branche (`cancel-in-progress`).
- **Clés de cache distinctes par job** : `lint` et `linux` partageant le même
  runner et le même profil debug, ils demandaient la même clé
  `actions/cache`. Le premier à la réserver la gagnait et l'autre échouait avec
  « Unable to reserve cache », donc sans cache du tout. Chaque job porte maintenant
  le sien dans la clé.

### Ajouts
- **Section d'installation dans le README** : un tableau des quatre archives
  publiées (archive autonome Linux, `.deb`, `Virial.app`, zip Windows), la
  commande `--install-desktop` et ce qu'elle enregistre, la vérification des
  sommes SHA-256, et l'avertissement Gatekeeper attendu sur le bundle macOS.
- **Job `cratesio` dans le workflow de publication** : il vérifie si le crate
  expose une cible `src/lib.rs` et lance `cargo publish --dry-run` le cas
  échéant. crates.io ne publie que des bibliothèques — virial-gpui est un
  binaire — donc le job le signale explicitement au lieu d'échouer sur une
  erreur opaque de l'API, et devient une vraie publication le jour où une cible
  `lib` est ajoutée.
- Métadonnées de manifeste complète pour un enregistrement éventuel :
  `repository`, `homepage`, `readme`, `keywords` (5, le maximum accepté) et
  `categories`.
- **Publication automatique** : un tag `v*` déclenche la construction et la
  publication des trois systèmes depuis un seul workflow. Chaque cible sort un
  paquet natif — `.deb` + archive `.tar.gz` pour Linux, `Virial.app` signé et
  compressé pour macOS, `.exe` + `.zip` pour Windows — accompagné d'un fichier
  `SHA256SUMS`.
  - `virial-gpui-<version>-linux-x86_64.tar.gz` : binaire strippé, entrée de
    bureau, icône et licence, pour une installation sans gestionnaire de paquets.
  - `Virial-<version>-macos-<arch>.app.tar.gz` : bundle `Virial.app` avec
    `Info.plist`, icône `.icns` générée par `iconutil` et signature ad hoc
    vérifiée par `codesign --verify --deep --strict`.
  - `virial-gpui-<version>-windows-x86_64.zip` : l'exe, le README et la licence.
  - Le contrôle de cohérence tag/`Cargo.toml` est centralisé dans un job
    `verify` unique, au lieu d'être répété — et contourné — dans chaque
    constructeur. Le build Windows vérifie le sous-système PE et échoue si le
    binaire n'est pas en `IMAGE_SUBSYSTEM_WINDOWS_GUI`, pour qu'une console
    ne puisse jamais clignoter au lancement.
  - Les trois constructeurs sont indépendants et un seul job `publish` crée la
    release : un échec Linux ne bloque plus la publication Windows, et les
    constructeurs ne se disputent plus `gh release create`.
- **Métadonnées Windows de l'exe** : le fichier s'affichait comme
  « virial-gpui » dans le volet des détails de l'Explorateur et dans la barre
  des tâches. La ressource de version embarque désormais le nom de produit
  « Virial », la société et le copyright, et le build de publication échoue si
  `ProductName` redevient le nom du crate.
- Le workflow accepte un lancement manuel (`workflow_dispatch`) pour
  vérifier les constructeurs sans publier de release.
- **Icônes de fichiers par type** : le navigateur et le panneau de détails
  choisissent l'illustration à partir du nom exact du fichier (`Dockerfile`,
  `.gitignore`, `Cargo.toml`, `LICENSE`, …) puis d'une table d'extensions
  couvrant langages, web, documents, données, archives, médias, polices et
  installeurs. Un nom inconnu retombe sur le type détecté par l'analyse, puis
  sur le document générique.
- **Barre de défilement latérale** : la liste de fichiers affiche un curseur
  proportionnel sur le bord droit, dont la position et la taille sont lues
  depuis la liste elle-même et ne peuvent donc pas diverger du contenu affiché.
- **Linux et macOS** : `renameat2(RENAME_NOREPLACE)` et
  `renameatx_np(RENAME_EXCL)` gardent le renommage atomique sans écrasement
  sur les deux systèmes ; les ports de transfert gagnent le clone copy-on-write
  de chaque plateforme (`FICLONE` sur Linux, `fclonefileat` sur macOS).
- **macOS** : le job CI `macos` exécute la suite complète et construit le
  binaire avec le backend Metal de gpui. gpui est désormais sélectionné par
  cible dans `Cargo.toml`, ce qui évite d'imposer Wayland/X11 à macOS.
- **macOS** : l'aperçu vidéo charge `libmpv.2` et la corbeille ne lit plus
  `/proc/self/mountinfo`, inexistant sur ce système.
- **Identité visuelle Windows complète** : `virial-gpui.exe` embarque désormais
  l'icône multi-résolutions (16/24/32/48/64/128/256) et un manifest
  PerMonitorV2. L'icône s'affiche dans l'installeur, sur le raccourci du
  bureau, dans Alt-Tab et sur la barre des tâches ; la fenêtre ne montre
  plus de console derrière le GUI au lancement. Activation via la feature
  Cargo `windows-resources` (active dans le job `windows` et le workflow
  de release, désactivée par défaut pour ne pas imposer `winres` aux builds
  Linux/macOS).
- **AppUserModelID au runtime** : `SetCurrentProcessExplicitAppUserModelID`
  est appelé à chaque lancement (`DeamonDev888.Virial.GPUI.1`), donc même
  un `virial-gpui.exe` lancé depuis un terminal se regroupe correctement
  dans la barre des tâches au lieu de prendre l'identité générique.
- **Associations de fichiers texte sous Windows** : `virial-gpui.exe
  --install-desktop` enregistre `Virial.GPUI.TextFile` comme ProgID +
  `DefaultIcon` pour `.env`, `.gitignore`, `.toml`, `.lock`, `.cfg`,
  `.ini` et `.properties` (sous `HKCU\Software\Classes`, sans droits
  admin). Les fichiers apparaissent avec l'icône Virial dans l'Explorateur
  et peuvent être ouverts via « Ouvrir avec → Virial » même si un autre
  éditeur est le défaut actuel ; une fonction `uninstall()` est disponible
  côté code pour nettoyer en un `reg delete`.
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
- Empty Trash from the context menu, with the same confirmation as the toolbar.
- Empty Trash from the toolbar with confirmation before permanent deletion.
- Folder sizes appear progressively, using a cancellable background scan with
  throttled filesystem work.
### Corrections
- **CI — tests instables en parallèle** : l'historique d'annulation prend un
  verrou exclusif `flock` sur `<data>/virial/undo/lock`. Les fixtures
  d'archives se recouvrent assez pour que l'exécution parallèle fasse perdre la
  course à
  `undoes_zip_edits_and_transfers_between_local_files_and_archives`, qui échouait
  avec « undo history is active in another window ». Les jobs Linux et Windows
  sérialisent maintenant leurs tests comme le job macOS le faisait déjà.
- **Curseur de défilement** : la hauteur minimale était appliquée après le
  calcul de la course, si bien qu'une liste très longue faisait sortir le
  curseur de sa piste (jusqu'à 23 px sur une piste de 512 px, l'écart croissant
  avec le contenu). La course est maintenant dérivée de la hauteur effective.
- **Curseur de défilement** : les mesures non finies (NaN, infini) n'étaient pas
  filtrées et pouvaient atteindre le rendu ; la hauteur était silencieusement
  réparée par `max`/`min` pendant que la position et le ratio restaient NaN.
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
- Suppress the harmless `dpkg-shlibdeps` warning for libc6's merged-`/usr` loader diversion
  when building `.deb` packages.
- Prepare code preview lines once and render only visible rows for responsive keyboard navigation.
- Keep preview textures alive until GPU rendering completes when switching files
  or opening a context menu, and cancel delayed previews on right-click.
- Keep large Linux Trash moves fast with desktop-created subdirectory permissions
  inside a private Trash root, and pause folder size scans during deletion.
- Revert the recent search memory changes and idle index unloading.
- Restore same-device Linux Trash items with persistent undo without copying or
  hashing their contents.
- Linux renames and same-device Trash moves keep persistent undo without copying
  or hashing file contents; undo preserves edits and refuses replaced items or
  occupied names.
- Ctrl-click uses the modifiers held at mouse-down and keeps multiple selection
  visible without opening the preview.
### Interface
- L'indicateur de connexion distante tient dans un carré de 22 px avec un
  cadre permanent : le nom d'hôte ne s'affiche plus en ligne (il était dans
  l'infobulle), ce qui évitait que la barre latérale se réorganise à chaque
  changement de connexion. La bordure s'accentue au survol et le bloc reste
  accenté plein quand la session est établie.
- L'indicateur remote `><` siège en tête de la ligne de pied de sidebar,
  séparé du libellé « LOCAL FILES » par un filet vertical ; la barre de statut
  séparée disparaît et la liste de fichiers descend jusqu'au bord.
- Group context actions with separators, shorten workspace labels, remove Refresh,
  and add Duplicate and a compression submenu for ZIP, TAR.GZ, TAR.XZ and TAR.BZ2.
- Replace the file kind column with a final modification date and time column.
- Compact search and filter icons reveal separate input fields below the toolbar;
  local filtering supports names and extensions.
