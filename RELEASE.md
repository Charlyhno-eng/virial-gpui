# Release procedure

How to publish a Virial release. The rule: **a merge into `main` only happens
with the CI fully green**, and **a release is always cut from `main`**.

## 0. What the CI enforces on every PR

Workflow `.github/workflows/ci.yaml`, three jobs that must pass before a merge:

| Job | Content |
|---|---|
| `lint` | `cargo fmt --all --check` · `cargo clippy --locked --all-targets -- -D warnings` · CHANGELOG entry required under « Non publié » (PR only) |
| `linux` | `cargo test --locked` · `.deb` build · benchmarks filesystem/indexed search |
| `windows` | `cargo test --locked` · release build · upload `virial-gpui-windows-x64` artefact |

Configure the GitHub repository settings so these checks are **required** for
merging (Settings → Branches → Branch protection on `main` → require status
checks `lint`, `linux`, `windows`). The repository itself cannot force this;
it is a one-time host-side setting.

## 1. Prepare the version

1. `git checkout main && git pull`
2. Bump `version` in `Cargo.toml`, then `cargo build` to refresh
   `Cargo.lock` (or run `cargo update -p virial-gpui` so only the package
   entry moves) and commit both files.
3. In `CHANGELOG.md`: rename `## [Non publié]` to `## [x.y.z] - YYYY-MM-DD`,
   and start a fresh empty `## [Unreleased/Non publié]` section above it
   (Keep a Changelog layout).
4. Commit: `chore(release): x.y.z`.

## 2. Pre-flight checks (all must pass, locally and on CI)

```bash
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

On Windows add the NASM toolchain for the gpui build
(`choco install nasm`); on Linux the build-deb native deps
(build-essential, pkg-config, dpkg-dev, desktop-file-utils, …).

## 3. Tag and push

```bash
git tag vX.Y.Z
git push origin main --tags
```

The tag **must equal `v` + the Cargo.toml version** — the release workflow
verifies it and fails otherwise.

## 4. What the release workflow publishes

Workflow `.github/workflows/release.yaml`, triggered by the `v*` tag:

- **Debian package** (ubuntu-24.04): `build-deb.sh` →
  `virial-gpui_x.y.z_amd64.deb` attached to the GitHub Release.
- **Windows executable** (windows-latest): `cargo build --release` →
  `dist/virial-gpui.exe` uploaded as `virial-gpui-windows-x64.exe` on the
  same GitHub Release.
- Release notes are generated from the tag; enrich them with the matching
  CHANGELOG section when publishing.

## 5. Post-release

1. Verify the GitHub Release shows both assets (.deb + .exe).
2. Smoke-test the artefacts on a clean machine.
3. If something is wrong: fix on `main`, move the tag
   (`git tag -d vX.Y.Z && git push origin :refs/tags/vX.Y.Z`, re-tag), the
   workflow re-uploads with `--clobber`.
