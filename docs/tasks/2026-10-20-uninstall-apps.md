# Uninstall applications

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done (compiled and unit-tested; no real package uninstalled yet)

## Context
The app installs and updates `.deb`/`.msi` packages but could only *stop tracking* an app
("Remove"), leaving the software installed. Wanted: an option to uninstall tracked
applications too.

## Decisions
- **Ubuntu**: `pkexec env DEBIAN_FRONTEND=noninteractive apt-get remove -y <package>`, same
  elevation, output streaming and exit-code mapping as install (`run_apt` now shared by
  install and remove). `remove`, not `purge`: keeps system-wide config like `apt remove`.
  Requires `package_name`; the name is re-validated (Debian charset **and** leading
  alphanumeric) so it can never be parsed as an apt-get option.
- **Windows**: look up the "Programs and Features" entry matching `windows_display_name`
  (or the display name) with `WindowsInstaller = 1`; its registry key name is the MSI
  product code. Run `msiexec /x {GUID} [/passive] /norestart /l*v <log>` elevated via the
  existing `ShellExecuteExW runas` path (`run_msiexec` now shared). The code must match
  `{8-4-4-4-12 hex}` before it reaches the command line. Exit codes: 0 → uninstalled,
  3010 → uninstalled + reboot, 1223/1602 → cancelled.
- New `Outcome` variants `Uninstalled` / `UninstalledRebootRequired` instead of reusing the
  `Installed*` ones, so notifications and UI text cannot mix them up.
- Uninstall takes the same `install_lock` and sets `installing`, so it never runs alongside
  an install and the UI disables the other buttons.
- **After a successful uninstall the app is disabled** (and `installed_version` re-detected,
  normally `None`, `skipped_version` cleared). Otherwise the next check would see "not
  installed" and offer the latest release as an update. The UI then asks whether to stop
  tracking it entirely.
- UI: "Uninstall" button on the Applications page, shown only when an installed version is
  known (and, on Ubuntu, a package name); confirmation dialog; live output while running,
  result + collapsible output afterwards. "Remove" stays "stop tracking" and its confirm
  text now points to Uninstall.

## Rejected alternatives
- Running the registry `UninstallString` for non-MSI installs: arbitrary command line from
  the registry, run elevated — out of scope (this tool only installs MSIs). The error tells
  the user to use Windows Settings > Apps instead.
- `apt-get purge` / `autoremove`: more destructive than users expect from "uninstall".
- Uninstall as part of "Remove": two separate intents; silent uninstall on remove is risky.

## What changed
- `crates/updater-core/src/installer/{mod,linux,windows}.rs`: `uninstall()`, `uninstall_deb`,
  `uninstall_msi`, shared `run_apt`/`run_msiexec`, `is_product_code` (+ test).
- `crates/updater-core/src/detect.rs`: `windows_entry_name`, `msi_product_code`, registry
  scan factored into `find_entry`.
- `src-tauri/src/commands.rs`, `lib.rs`: `uninstall_app` command.
- `src/lib/{api,types}.ts`, `src/components/Apps.svelte`: Uninstall UI.
- `README.md`: feature bullet.

## Evidence
- `cargo test -p updater-core`: 26 passed.
- `cargo clippy --workspace --all-targets` (Linux) and
  `cargo clippy -p updater-core --all-targets --target x86_64-pc-windows-gnu`: no issues.
- `npm run check`: 0 errors, 0 warnings.

## Corrections
None.

## Left undone
- Not exercised end-to-end: no real package was removed on Ubuntu or Windows.
- If the app also has a pending update on the Updates page, that card shows "Installing…"
  while the uninstall runs (it reads the shared `installing` flag); the pending entry is
  dropped once the uninstall succeeds.

## How to verify
1. `npm run tauri dev`, track an app whose `.deb` is installed (package name set).
2. Applications → Uninstall → confirm → polkit prompt → output streams, "was uninstalled".
3. `dpkg -s <package>` reports not installed; app shows Enabled unchecked; answering
   OK to the follow-up removes it from the list.
4. Cancel the polkit prompt → "Uninstall cancelled.", nothing changes.
5. Windows: same with an MSI-installed app; check the log under the app log directory.
