# Automatic updates option

- **Date:** 2026-10-21
- **Branch:** `feat/auto-update`
- **Status:** done (not run live against a real release)

## Context
Every update needed a click on Install (or Update all). Users asked for an option to install updates as soon as a check finds them.

## Decisions
- Global setting `Settings.auto_update` ("Install updates automatically"), **off by default** so existing behaviour and the "you stay in control" promise hold unless the user opts in.
- Per-app `TrackedApp.auto_update`, default **true** ("Update automatically" in the app editor): only matters when the global switch is on, and lets the user exclude individual apps. `serde(default)` keeps old config files and backups loading.
- Elevation is unchanged: automatic installs still go through `pkexec` / UAC, so the OS prompt appears unattended. Truly silent installs would need a polkit rule or a privileged service — a security decision, not made here.
- `install_update`'s body moved to `commands::install_pending(app, id, wait)`; the command passes `wait = false` (fails fast as before), the automatic path passes `wait = true` and queues behind a running manual install. The pending entry is now read *after* taking the install lock, so a queued install sees the current state.
- `run_check` computes the auto-eligible updates, leaves them out of the "Update available" notification and the open-window trigger (the install itself notifies installed/failed), and spawns `auto_install` so `check_now` still returns to the UI right away. An `auto_installing` flag prevents two batches overlapping.
- A cancelled or failed automatic install records `(app, tag)` in `AppState.auto_declined` and is not retried automatically for that tag until restart — otherwise a dismissed password/UAC prompt would come back at every check. Manual Install still works; a newer tag is tried again.
- New `install-finished` event (stage + message) emitted by every install, so the UI shows the result of installs it did not start. `App.svelte` sets the progress stage and reloads settings (installed version changed).

## Rejected alternatives
- Per-app opt-in only (no global switch): more clicks for the common "update everything" case.
- Retrying declined/failed installs at each check: repeats the elevation prompt every interval.
- Awaiting installs inside `run_check`: would block the Check now button for the whole install batch.

## What changed
- `crates/updater-core/src/config.rs`: `Settings.auto_update`, `TrackedApp.auto_update`, defaults test.
- `src-tauri/src/commands.rs`: `install_pending`, `FinishedEvent` / `install-finished`.
- `src-tauri/src/lib.rs`: auto-eligible selection in `run_check`, `auto_install`.
- `src-tauri/src/state.rs`: `auto_installing`, `auto_declined`.
- `src/lib/types.ts`, `src/App.svelte`, `src/components/Settings.svelte`, `src/components/AppEditor.svelte`: toggles and event listener.
- `scripts/screenshots/demo.ts`: fixture fields.
- `README.md`: feature bullet no longer claims installs always need a click.

## Evidence
- `cargo test --workspace`: 32 passed.
- `cargo clippy --workspace --all-targets`: no issues.
- `npm run check`: 0 errors, 0 warnings.

## Left undone
- Not exercised live (no real check → auto install run).
- Network/download errors are also remembered as declined for that tag; a transient failure is retried only after restart or by clicking Install.
- No silent (prompt-free) install mode.
- "Update all" started while an automatic batch runs fails fast on the lock for those apps (same as clicking two Installs).

## How to verify
1. Settings → tick *Install updates automatically* → Save.
2. Track an app with an older installed version, Check now: the install starts by itself, the OS prompt appears, Updates page shows progress, a notification reports the result.
3. Cancel the prompt: no new prompt at the next Check now for the same tag; Install button still works.
4. Untick *Update automatically* on an app: it is only announced, not installed.
