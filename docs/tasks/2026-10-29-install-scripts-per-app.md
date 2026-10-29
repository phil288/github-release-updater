# Install releases with a project's install script

- **Date:** 2026-10-29
- **Branch:** `feat/install-script`
- **Status:** done (not clicked live in the app)

## Context
Some projects publish releases on GitHub but expect users to install through a
script on their own site, e.g. Zed: `curl -f https://zed.dev/install.sh | sh`.
The app could only install `.deb` / `.msi` release assets, so such apps were
reported as "no matching asset". Goal: keep detecting new releases from GitHub
and, when one appears, install it by running the project's script.

## Decisions
- New per-app, per-OS setting `install_script: { linux, windows }` (HTTPS URL).
  `AssetPatterns` was generalised to `PerOs` (type aliases `AssetPatterns`,
  `InstallScripts`) so both share `for_os` trimming. Old configs load unchanged
  (`serde(default)`), so `SCHEMA_VERSION` stays 1.
- Release detection is untouched (`check::evaluate`). When a script is set for the
  current OS, asset selection no longer gates the update: `UpdateInfo.asset` is now
  `Option<Asset>` (`None` for script installs) and `UpdateInfo.install_script`
  carries the URL.
- `script::fetch`: HTTPS only, redirects must stay HTTPS, any host (the user named
  it), fails on HTTP errors like `curl -f`, rejects empty or > 1 MiB bodies. The
  script is saved to the cache dir (`<repo>/<tag>/install.sh|ps1`) and its SHA-256
  logged, since nothing published exists to verify it against.
- `installer::run_script`: `/bin/sh install.sh` on Linux,
  `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File install.ps1`
  on Windows (`CREATE_NO_WINDOW`). Runs as the current user, no elevation, stdin
  closed; exit 0 → `Installed`, else `Failed` with the last output line.
- The apt output-streaming loop moved from `installer/linux.rs` into a shared
  `installer::run_streaming`, used by both apt and scripts.
- After a successful script install `installed_version` is set from the tag (no
  package database to read).

## Rejected alternatives
- Piping into `sh` exactly like `curl | sh`: saving first gives a file to hash,
  log and inspect, and avoids running a truncated download.
- Running scripts elevated (pkexec / UAC): scripts like Zed's install per user into
  `~/.local`; elevating would install into root's home. Scripts that need `sudo`
  will fail (no terminal) and show that in the log.
- Restricting script hosts to an allowlist: the host is project-specific, and the
  user types the URL explicitly. HTTPS is enforced instead.
- A "version command" (e.g. `zed --version`) for detecting the installed version:
  useful, but extra scope; left undone.

## What changed
- `crates/updater-core/src/config.rs` — `PerOs`, `TrackedApp.install_script`,
  `install_script(os)`, validation (HTTPS, trimming), test.
- `crates/updater-core/src/script.rs` (new) — `check_url`, `fetch`, `file_name`.
- `crates/updater-core/src/check.rs` — `UpdateInfo.asset: Option`, `install_script`;
  `Preview.install_script`; `run_install_script` branch in `download_and_install`; test.
- `crates/updater-core/src/installer/{mod,linux}.rs` — `run_script`, `run_streaming`; test.
- `crates/updater-core/examples/probe.rs` — handles optional asset.
- `src/lib/types.ts`, `src/components/AppEditor.svelte` (two URL fields, preview
  shows the script), `src/components/Updates.svelte` (shows script instead of asset),
  `scripts/screenshots/demo.ts`.

## Evidence
- `cargo test -p updater-core`: 36 passed (new: script URL validation, pasted-command normalisation, config
  validation, `install_script_replaces_asset`, `runs_script_and_maps_exit_code`
  which runs real `sh` scripts for exit 0 and exit 3).
- `cargo clippy --workspace --all-targets`: clean. `cargo check -p updater-core
  --target x86_64-pc-windows-gnu`: ok. `npm run check`: 0 errors.
- One-off run of `script::fetch("https://zed.dev/install.sh")`: 5368 bytes saved,
  SHA-256 logged; a missing URL returned `Status(404)`.

## Corrections
- First version stored the field as typed. In the app the user entered
  `https://zed.dev/install.sh | sh`; `url::Url::parse` accepted it and
  percent-encoded the spaces, so the fetch went to `install.sh%20|%20sh` → 404.
  Fix: `script::normalize` extracts the URL from a pasted command (`curl -f URL | sh`,
  `irm URL | iex`, quoted URLs) on save/load and again before fetching (for values
  already in memory); `check_url` now rejects whitespace.

## Left undone
- Not clicked through in the running app; the Zed script was fetched but not executed.
- Installed-version detection for script installs (version command). Until then, an
  app added without a version is offered the latest release once; set the version
  in the editor to avoid that.
- With *Install updates automatically* on, script installs run without any OS
  prompt (asset installs still show polkit/UAC). Per-app opt-out still applies.
- Uninstall does not know how to remove script-installed apps.
- PowerShell path is compile-checked only (windows-gnu), not run on Windows.

## How to verify
```bash
cargo test -p updater-core
npm run check
```
Manual: add `zed-industries/zed`, set Ubuntu install script to
`https://zed.dev/install.sh`, click *Test match* (shows the script), set installed
version to an older one, *Check now*, *Install*; the log shows the script SHA-256
and Zed's output, and `~/.local/bin/zed --version` reports the new version.
