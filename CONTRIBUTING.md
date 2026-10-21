# Contributing

Thanks for helping. Bug reports, repositories whose assets are not detected, and pull
requests are all welcome.

## Before you start

* For anything larger than a small fix, open an issue or a discussion first, so we agree
  on the approach before you spend time on it.
* Security problems go to [Security Advisories](https://github.com/phil288/github-release-updater/security/advisories/new),
  not to public issues. See [SECURITY.md](SECURITY.md).

## Set up

Ubuntu:

```bash
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev build-essential pkexec
```

Windows: Visual Studio Build Tools with the C++ workload. WebView2 is preinstalled on Windows 10/11.

Both: Rust stable via [rustup](https://rustup.rs) and Node.js 22.

```bash
npm install
npm run tauri dev
```

## Project layout

| Path | Contents |
|---|---|
| `crates/updater-core` | GUI-free logic: repo parsing, versions, asset selection, GitHub client, download + checksum, installers, backup. Most changes and all tests live here. |
| `src-tauri` | Tauri 2 shell: tray, scheduler, notifications, commands. Keep it thin. |
| `src` | Svelte 5 UI (Updates, Applications, Settings). |
| `scripts` | Install / uninstall scripts and the screenshot harness. |
| `docs/tasks` | Decision records, one per change. |

## Checks

Run these before opening a pull request; CI runs the same on Ubuntu and Windows:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check
```

Probe how a repository's assets are selected, without installing anything:

```bash
cargo run -p updater-core --example probe -- owner/name --download
```

## Pull requests

* One topic per pull request, with tests for logic changes in `updater-core`.
* Describe how you tested it, and on which OS. Windows and Ubuntu behave differently around
  installers; say if you could only test one.
* For a notable change, add a short decision record in `docs/tasks/` (copy `_TEMPLATE.md`):
  what you chose, what you rejected and why.
* UI changes: update the screenshots with `scripts/screenshots/capture.sh` (needs `npm run dev`).

## Releasing (maintainers)

Bump the version in `package.json`, `Cargo.toml` and `src-tauri/tauri.conf.json`, then push
a `vX.Y.Z` tag. The release workflow builds the `.deb` and `.msi` and publishes the release.

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).
