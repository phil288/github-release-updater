# Install scripts

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done (Windows script not executed)

## Context
The app needs a one-command install on Ubuntu and Windows. There is no published release
yet, so the default has to be building from source.

## Decisions
- `scripts/install.sh` (Ubuntu/Debian) and `scripts/install.ps1` (Windows), three modes each:
  build from source (default), `--release OWNER/REPO` / `-Release` (latest published package),
  `--package FILE` / `-Package` (existing file).
- Source build installs missing dependencies: apt packages and rustup on Linux; MSVC build tools,
  rustup and Node LTS via winget on Windows. Opt out with `--skip-deps` / `-SkipDeps`. On Linux,
  Node is checked (20+) but not installed, because it is usually managed by nvm.
- `tauri build --bundles deb|msi` builds only the native package, so the `["deb","msi"]` targets in
  `tauri.conf.json` don't break either platform.
- The scripts run unprivileged and elevate only for the install: `sudo apt-get install` (resolves
  dependencies; the file is copied to a 0755 `/tmp` dir for `_apt`) and
  `Start-Process msiexec -Verb RunAs` (UAC prompt, `/passive`, verbose log in `%TEMP%`).
  `install.sh` refuses to run as root.
- Release mode uses the same safeguards as the app: arch-matched asset, download URL must be
  `https://github.com/`, SHA-256 checked against the API `digest` when present.

## Rejected alternatives
- `curl | bash` one-liner from a URL: there is no published repo/release yet.
- Auto-installing Node on Linux: it conflicts with nvm setups.
- `dpkg -i`: does not pull dependencies.

## What changed
`scripts/install.sh`, `scripts/install.ps1`, `README.md` (Install section).

## Evidence
- `bash -n` passes. Error paths exit 1: missing package, malformed repo, unknown option.
- Release mode against `storytold/wordcraft` with a stub `sudo`: picked
  `wordcraft-0.8.0-linux-x86_64.deb`, downloaded 96 MB, `SHA-256 verified`, called
  `apt-get install -y /tmp/gh-release-updater-install.*/…deb`, and removed the temp dir.
- The asset picker resolves amd64, arm64 and riscv64 correctly, and reports an error for i386.

## Corrections
- None.

## Left undone
- `install.ps1` has not been run: there is no Windows machine or pwsh here. It needs one run on Windows.
- Source mode has not been run end-to-end: the WebKitGTK dev packages are still missing on this machine.
- No uninstall mode. Use `sudo apt remove <package>` or Windows "Installed apps".

## How to verify
- Ubuntu: `scripts/install.sh`, then launch "GitHub Release Updater" from the app menu.
- Windows: `powershell -ExecutionPolicy Bypass -File scripts\install.ps1` → UAC prompt → app in Start menu.
