# Uninstall scripts

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done (Windows script not executed)

## Context
Counterpart to `scripts/install.sh` / `install.ps1`: remove the app cleanly from Ubuntu and Windows.

## Decisions
- `scripts/uninstall.sh` and `scripts/uninstall.ps1`. They show what will happen, ask for
  confirmation (`-y` / `-Yes` skips it), then: stop the running app, remove the package with
  elevation, and remove the per-user login item. `--purge` / `-Purge` also deletes the config dir
  (settings + token), cache (downloads) and data/log dirs.
- **Package name is detected, not hard-coded.** Tauri derives the Debian name by kebab-casing the
  product name, which gives `git-hub-release-updater`. The script asks dpkg which package owns
  `/usr/bin/gh-release-updater`. On Windows the MSI product codes are found in the Uninstall
  registry keys (HKLM 64/32-bit, HKCU) by `DisplayName == "GitHub Release Updater"` and removed with
  `msiexec /x {code}` (elevated, passive, logged).
- **Login item removed explicitly.** The autostart plugin writes
  `~/.config/autostart/GitHub Release Updater.desktop` / `HKCU\…\Run` (plus `StartupApproved\Run`).
  These belong to the user, not the package, so package removal alone would leave a dangling entry.
- `apt-get remove`, not `purge`: the package has no system-wide config; user data is handled by `--purge`.
- Process match uses `pgrep -f '^/usr/bin/gh-release-updater( |$)'`. `pgrep -x` would never match,
  because Linux truncates process names to 15 characters (the binary name has 18).
- The Linux script refuses to run as root, because the per-user paths would point at root's home.

## Rejected alternatives
- Renaming `productName` to get a nicer Debian package name: it would also change the menu entry,
  window/tray labels and MSI display name.

## What changed
`scripts/uninstall.sh`, `scripts/uninstall.ps1`, `README.md` (Uninstall section).

## Evidence
- First real `npx tauri build --bundles deb`: `GitHub Release Updater_0.1.0_amd64.deb` (4.3 MiB),
  `Package: git-hub-release-updater`, binary `/usr/bin/gh-release-updater`,
  `Depends: pkexec, libayatana-appindicator3-1, libwebkit2gtk-4.1-0, libgtk-3-0`.
- Purge path tested against temporary `XDG_*` dirs: the login item and all three app dirs were
  deleted, and the abort path exits 1 without changes.

## Corrections
- **Incident during testing:** the first test run assumed the app was not installed. It was
  installed (by the user), so the script correctly detected `git-hub-release-updater` and
  **stopped the user's running instance** before `sudo` failed (no terminal). The package, settings
  and login item were not touched (verified with `dpkg -s` and `ls`). The app was restarted with
  `--minimized`. Later tests used a copy of the script pointing at a non-existent binary.

## Left undone
- `uninstall.ps1` has not been run (no Windows machine / pwsh here).
- The real `apt-get remove` was not run, to avoid removing the user's installation.

## How to verify
- Ubuntu: `scripts/uninstall.sh` → confirm → the package is gone (`dpkg -s git-hub-release-updater` fails),
  and `~/.config/autostart/GitHub Release Updater.desktop` is removed.
- Windows: `powershell -ExecutionPolicy Bypass -File scripts\uninstall.ps1` → UAC → the app is gone
  from "Installed apps", and the startup entry is removed.
