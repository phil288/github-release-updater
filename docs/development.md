# Development

See [CONTRIBUTING.md](../CONTRIBUTING.md) for setup, checks and pull request guidelines.

## Install from source

The scripts build from source by default (installing missing build dependencies),
then install the package with apt (sudo password) or msiexec (UAC prompt).

```bash
scripts/install.sh
```

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1
```

Other modes: `--release OWNER/REPO` / `-Release OWNER/REPO` installs the latest published
package of that repository (SHA-256 checked against GitHub's digest), and
`--package FILE.deb` / `-Package FILE.msi` installs a package you already have.
`--skip-deps` / `-SkipDeps` skips installing build dependencies.

## Uninstall with the scripts

```bash
scripts/uninstall.sh
```

```powershell
powershell -ExecutionPolicy Bypass -File scripts\uninstall.ps1
```

The scripts stop the app, remove the package (sudo / UAC) and remove your login item, which
belongs to your user and not to the package. Settings are kept unless you pass `--purge` /
`-Purge`, which also deletes settings, token, downloads and logs. Pass `-y` / `-Yes` to skip
the confirmation.

## Build installers

```bash
npm run tauri build
```

Outputs `target/release/bundle/deb/*.deb` on Ubuntu and `target/release/bundle/msi/*.msi` on Windows.
The `.msi` must be built on Windows (or by the release workflow).

## Screenshots

`scripts/screenshots/` runs the real UI in a browser with mocked Tauri IPC and demo data:

```bash
npm run dev
scripts/screenshots/capture.sh
```

It writes `assets/screenshots/*.png` and `assets/social-preview.png` (the 1280×640 image for
*Settings → General → Social preview* on GitHub).

## Release

`.github/workflows/release.yml` runs on GitHub-hosted runners. Pushing a version tag builds
the `.deb` (Ubuntu 22.04, so it installs on 22.04 and later) and the `.msi` (Windows), then
publishes a GitHub release with both packages and `SHA256SUMS.txt`:

```bash
git tag v0.1.0
git push origin v0.1.0
```

* The tag must equal the version in `package.json`, `Cargo.toml` and `src-tauri/tauri.conf.json`,
  otherwise the run fails. A tag with a suffix (`v0.2.0-rc.1`) becomes a pre-release. MSI only
  accepts numeric versions, so pre-release tags need care.
* Running the workflow manually (*Actions → release → Run workflow*) builds both packages as
  artifacts and does not publish a release.
* Assets are named `gh-release-updater_<version>_amd64.deb` and `gh-release-updater_<version>_x64.msi`,
  so the app can track its own repository.
* `ci.yml` runs clippy and tests on Ubuntu and Windows for every push and pull request.
* `pages.yml` publishes `site/` to GitHub Pages when it or `assets/` change on `main`.
* The packages are unsigned, so Windows SmartScreen warns on first install.

## Data locations

| | Ubuntu | Windows |
|---|---|---|
| Settings | `~/.config/dev.phil.gh-release-updater/config.json` | `%APPDATA%\dev.phil.gh-release-updater\config.json` |
| Token | `…/github-token` (mode 0600) | same folder |
| Downloads | `~/.cache/dev.phil.gh-release-updater/downloads` | `%LOCALAPPDATA%\dev.phil.gh-release-updater\downloads` |
| MSI logs | — | `%LOCALAPPDATA%\dev.phil.gh-release-updater\logs` |
