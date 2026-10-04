![Virial logo](assets/images/virial-banner.png)

# Virial
---

Virial is a Linux file manager for browsing local folders, mounted drives, and recent files. Navigate with the sidebar and breadcrumbs, organize related folders into persistent workspaces, and preview supported images and text files. The browser also supports common file actions, including selecting, copying, moving, renaming, and trashing files.

Press `Ctrl+P` to open the global path picker. It searches accessible locations as you type and ranks fuzzy matches in file names and paths, making it a zoxide-like way to jump quickly to a folder or file. Virial performs this search itself, so it does not require zoxide or a prebuilt search index; arrow keys move through results and Enter opens the selection.

## Quickstart

### Install from Git

```sh
cargo install --git https://github.com/Charlyhno-eng/virial-gpui.git --locked
```

This fetches and builds Virial without requiring a local checkout. Cargo installs the executable to `~/.cargo/bin`; run it with:

```sh
virial-gpui
```

To add a desktop launcher and application icon, clone the repository and run its installer instead:

```sh
./install.sh
```
