# Virial GPUI

Virial is a file manager for local and remote files on Linux, macOS and Windows.

## Features

- Browse local folders, mounted drives, recent files, ZIP archives, and SSH/SFTP hosts.
- Search across locations and preview supported images, PDFs, media and text files.
- Group folders into persistent workspaces.
- Select, copy, move, rename, and trash files with recoverable operations.

## Working rules

- Follow existing conventions and keep changes focused.
- Keep user-facing documentation concise and in English.
- Update these instructions only when lasting project guidance changes.
- **Do not edit `.github/workflows/` casually**: the job set is guarded
  structurally by the `lint` job (a missing job fails the merge), and
  `release.yaml` publishes real releases — read `RELEASE.md` before touching
  either workflow.

## Changelog (mandatory)

Any pull request that changes user-facing behavior, the CI, or the packaging
must add an entry to `CHANGELOG.md` under « Non publié », using the existing
section categories (Ajouts / Corrections / Interface). Refactors with no
observable effect may skip it.

## Releases

A release is a version bump in `Cargo.toml` merged into a green `main` — the
workflow builds all three platforms, tags `v<version>` and publishes
everything automatically. Never push release tags by hand; the full procedure
lives in `RELEASE.md`.
