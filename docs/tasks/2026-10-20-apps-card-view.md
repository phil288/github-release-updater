# Applications card view

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done (not checked visually in the running app)

## Context
The Applications page only had full-width rows. User asked for a card view, with the chosen layout remembered across app restarts.

## Decisions
- Layout stored in `Settings.apps_view` (`"list"` | `"cards"`, enum `AppsView` in `crates/updater-core/src/config.rs`), persisted to the same settings file as everything else. `#[serde(default)]` → existing config files load as `list`; no schema bump needed.
- List/Cards segmented toggle next to the page heading; clicking saves immediately via `save_settings`. Hidden while the app editor is open.
- Cards: CSS grid `repeat(auto-fill, minmax(260px, 1fr))`; same card markup as list, name/repo on top, controls wrap below. Uninstall progress/result still spans full card width.
- `Settings.svelte` save now takes `apps_view` from the store (like `apps`) so saving the Settings page cannot revert a layout change made after its draft was snapshotted.

## Rejected alternatives
- `localStorage`: simpler, but WebView storage is separate from the settings file the user backs up, and can be wiped independently. Settings file is the single source of truth here.

## What changed
- `crates/updater-core/src/config.rs` — `AppsView` enum, `apps_view` field + default.
- `src/lib/types.ts` — `AppsView`, `Settings.apps_view`.
- `src/components/Apps.svelte` — toggle, grid layout, scoped styles.
- `src/components/Settings.svelte` — preserve `apps_view` on save.

## Evidence
- `cargo test -p updater-core`: 26 passed.
- `cargo check --workspace`: ok.
- `npm run check`: 0 errors, 0 warnings. `npm run build`: ok.

## Corrections
None.

## Left undone
- Not visually verified in the Tauri window.
- Layout is included in backup export/import (part of `Settings`); merge import takes the imported value, like other settings.

## How to verify
`npm run tauri dev` → Applications → click **Cards** → close and reopen app → still cards. Check `apps_view` in the settings JSON.
