# Clicking the tray icon opens the window

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done (builds; click not exercised against a running panel)

## Context
Double-clicking the tray icon on Ubuntu did nothing. A left-click handler already
existed in `build_tray`, but Tauri's default Linux tray backend
(libappindicator) never emits `TrayIconEvent`s — tray-icon documents click
events as unsupported there. Only the menu worked.

## Decisions
- Enable tray-icon's `ksni` feature on Linux (`src-tauri/Cargo.toml`). KSNI
  serves the StatusNotifierItem D-Bus interface itself; its `Activate` (left
  click) maps to `TrayIconEvent::Click { Left, Up }` and `SecondaryActivate`
  (middle click) to `Click { Middle }`. When both `libappindicator` (pulled by
  tauri) and `ksni` are enabled, tray-icon picks KSNI (confirmed with
  `cargo tree -e features -i tray-icon`).
- Handler also matches `TrayIconEvent::DoubleClick { Left }` (Windows-only event).
  A double click on Windows also produces single clicks, so opening on either is
  idempotent.

## Rejected alternatives
- Single-click-only vs double-click-only: StatusNotifierItem has no double-click
  signal, so Linux cannot distinguish them. Any left click / activate opens the
  window; a double click therefore opens it too.
- Forking Tauri / calling libappindicator `secondary_activate_target` directly:
  more code, only covers middle click.

## What changed
- `src-tauri/Cargo.toml`: Linux-only `tray-icon = { version = "0.25", default-features = false, features = ["ksni"] }`.
- `src-tauri/src/lib.rs` `build_tray`: tray event handler accepts left Click-Up and left DoubleClick.

## Evidence
- `cargo build -p gh-release-updater` succeeds.
- `cargo tree` shows tray-icon with both `ksni` and `libappindicator` features → KSNI backend.
- Live test skipped: the installed `/usr/bin/gh-release-updater` was running, and
  the single-instance plugin makes a second (debug) instance exit.

## Corrections
- Initial assumption that the existing left-click handler should already work on
  Ubuntu was wrong: the AppIndicator backend emits no click events at all.

## Left undone
- GNOME panel behaviour depends on the Ubuntu AppIndicator extension, which calls
  `Activate` on left click for items that are not menu-only. Right click shows the menu.

## How to verify
1. Quit the installed instance (tray → Quit).
2. `cargo tauri dev` (or build/install the .deb).
3. Close the window, then click / double-click the tray icon → window shows.
4. Right-click → menu still appears.
