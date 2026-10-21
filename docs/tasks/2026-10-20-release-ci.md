# Self-hosted release CI

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done (no repo/runners yet; workflows not executed)

## Context
Releases of this app should be built on the user's own machines (self-hosted runners). Each
release needs the `.deb` and `.msi` attached to a GitHub release.

## Decisions
- `release.yml`: trigger on `v*.*.*` tags plus `workflow_dispatch` (build only, no release).
  Jobs: `prepare` (version check) → `linux` (.deb) and `windows` (.msi) in parallel → `release`.
- **Version guard**: `package.json`, `Cargo.toml` (workspace) and `tauri.conf.json` must agree, and
  the tag must be `v<version>`. This prevents publishing packages with a wrong embedded version.
  Tags with a hyphen are published as pre-releases.
- **Stable asset names** `gh-release-updater_<ver>_<debarch>.deb` / `_x64.msi`: Tauri's default
  names contain spaces (`GitHub Release Updater_0.1.0_amd64.deb`). The new names contain arch
  tokens the app's own asset picker understands, so the app can update itself.
- `SHA256SUMS.txt` is attached, and GitHub also exposes per-asset digests. The app and the
  install scripts verify either.
- `softprops/action-gh-release@v2` publishes the release. It is a JS action, so it does not need `gh` on the runner.
  `contents: write` is granted only to that job; everything else gets `contents: read`.
- **No toolchain installers in the workflow**: self-hosted runners should not run `sudo apt` or
  `rustup` installs on every run. The workflow checks for Rust and the WebKitGTK libraries and fails
  with the install command if they are missing. Node comes from `actions/setup-node`, which needs no root.
- `dtolnay/rust-toolchain` is not used: it runs bash steps, and a Windows self-hosted runner may not
  have bash.
- `ci.yml` (renamed from `build.yml`) moved to self-hosted. Pull requests from forks are skipped,
  because running fork code on personal machines is a security risk.
- `--bundles deb|msi` per platform; tests (`cargo test --locked`, `npm run check`) run before the Linux build.

## Rejected alternatives
- Injecting the version from the tag at build time: MSI requires numeric versions, and a silent
  mismatch with the repository versions is worse than a clear failure.
- Build caching via a persistent `CARGO_TARGET_DIR`: `actions/checkout` cleans the workspace. A
  shared target dir outside the workspace is possible but was left out until build times matter.
- Code signing: no certificate available.

## What changed
`.github/workflows/release.yml` (new), `.github/workflows/ci.yml` (was `build.yml`, now self-hosted),
`README.md` (Release section and runner setup).

## Evidence
- Both workflow files parse as YAML. They have not been run: no GitHub repository or runners
  exist yet, and `actionlint` is not installed here.

## Corrections
- None.

## Left undone
- First real run on registered runners.
- MSI signing and a Linux arm64 build (needs an arm64 runner).
- A version-bump helper script.

## How to verify
1. Register runners as described in README → Release → Runner setup.
2. Actions → release → Run workflow: both jobs succeed and `deb` and `msi` artifacts appear.
3. `git tag v0.1.0 && git push origin v0.1.0`: the release appears with 2 packages and `SHA256SUMS.txt`.
4. In the app, track this repository: Test match picks `gh-release-updater_0.1.0_amd64.deb` / `_x64.msi`.
