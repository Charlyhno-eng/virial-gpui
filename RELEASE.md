# Release procedure

How a Virial release ships. The rule: **a release is a version bump merged
into a green `main`** — everything else (three platform builds, the tag, the
GitHub Release) happens automatically.

## 0. What the CI enforces on every PR

Workflow `.github/workflows/ci.yaml`, five jobs:

| Job | Content |
|---|---|
| `lint` | Actions workflow validation (`actionlint`) · `cargo fmt --all --check` · `cargo clippy --locked --all-targets -- -D warnings` · job-set guard (the five jobs must exist in `ci.yaml`) · CHANGELOG entry under « Non publié » (PR only) |
| `linux` | `cargo test --locked` (debug) |
| `windows` | `cargo test --locked` (debug) |
| `macos` | `cargo test --locked` (debug, Metal backend) |
| `package-and-benchmarks` | `.deb` build + filesystem/indexed-search benchmarks (release profile, only on `main` or labeled PRs) |

Configure the GitHub repository settings so `lint`, `linux`, `windows` are
**required** checks for merging (Settings → Branches → Branch protection on
`main`). The repository itself cannot force this; it is a one-time host-side
setting.

## 1. Publish a release

1. Bump `version` in `Cargo.toml`, refresh `Cargo.lock` (`cargo update -p
   virial-gpui` moves only the package entry), and commit both files.
2. In `CHANGELOG.md`: rename `## [Non publié]` to `## [x.y.z] - YYYY-MM-DD`
   and start a fresh empty `## [Non publié]` section above it. Add a minimal
   bump entry under the fresh « Non publié » — the `lint` job requires it.
3. Open a pull request with those changes. A convenient title:
   `release: bump version to x.y.z`.
4. Merge once the checks are green. Done — no tag to push, nothing else.

The version number is the whole decision: a merge **without** a bump
publishes nothing. `v0.1.1` already exists → merging docs or fixes is
invisible to the release machinery.

## 2. What happens after the merge

Workflow `.github/workflows/release.yaml` triggers on the CI finishing on
`main` (`workflow_run`):

1. A `prepare` job reads `Cargo.toml` and publishes only when **the CI
   succeeded and the version has no tag yet**; otherwise it skips everything.
2. `linux`, `macos`, `windows` build the release profile **in parallel** on
   the exact commit the CI validated (~45–60 min total, not per platform).
3. `publish` verifies all three artifact sets are present, builds
   `SHA256SUMS`, creates the `v<version>` tag pinned to that commit, and
   publishes the GitHub Release with generated notes.

A manual `workflow_dispatch` run builds all three platforms and uploads
artifacts for inspection **without publishing anything** — useful to check
the builders before a release.

## 3. The release assets

| Asset | Platform |
|---|---|
| `virial-gpui_x.y.z_amd64.deb` | Debian/Ubuntu, installs to `/usr/bin` with a launcher |
| `virial-gpui-x.y.z-linux-x86_64.tar.gz` | Standalone Linux: binary, desktop entry, icon |
| `Virial-x.y.z-macos-<arch>.app.tar.gz` | macOS app bundle, ad-hoc signed |
| `virial-gpui-x.y.z-windows-x86_64.zip` (+ plain `.exe`) | Windows x86_64, GUI subsystem verified |

## 4. Post-release

1. Check the release page lists the four assets plus `SHA256SUMS`.
2. Smoke-test the artifacts on a clean machine.
3. If something is wrong: fix on `main` and bump to a **new** version number
   (the workflow refuses to reuse an existing tag). There is deliberately no
   partial-release path: a failed builder must be fixed, not shipped around.
