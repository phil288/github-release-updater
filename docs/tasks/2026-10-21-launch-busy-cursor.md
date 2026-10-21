# Endless busy cursor when reopening the app

- **Date:** 2026-10-21
- **Branch:** `feat/initial-app`
- **Status:** done (verified with Wayland protocol logs, not clicked live in the dock)

## Context
Reported on Ubuntu 26.04, GNOME on Wayland: the app felt slow to launch, and after
minimising (or closing the window to the tray) and opening it again from the
launcher, the mouse kept showing the loading cursor.

## Decisions
- Root cause: GNOME gives every launch an XDG activation token and shows the busy
  cursor until a window is activated with it. When the app is already running, the
  new process exits inside `tauri-plugin-single-instance` and its token is never
  used. The running instance raises its window with a fresh token GTK requests
  itself, so GNOME's launch sequence stays open.
- Fix (`src-tauri/src/activation.rs`): every launch writes its token
  (`XDG_ACTIVATION_TOKEN`, else `DESKTOP_STARTUP_ID`) to
  `$XDG_RUNTIME_DIR/dev.phil.gh-release-updater.activation-token`, or deletes the
  file when it has none. The single-instance callback, on the main thread, reads and
  deletes it and hands it to GDK (`gdk_wayland_display_set_startup_notification_id`,
  or `gdk_notify_startup_complete_with_id` on X11) before `show_window`, so the next
  focus activates with the launcher's token.
- Raw FFI to libgdk-3 / libgobject instead of adding `gtk`/`gdkwayland-sys` crates:
  four symbols, already linked through Tauri.

## Rejected alternatives
- Passing the token as an argument: the plugin forwards the second process's real
  argv, which cannot be changed without re-executing the binary.
- Calling the plugin's D-Bus method ourselves: duplicates the plugin's private
  naming scheme.
- `enable_gtk_app_id` / renaming the desktop file: not the cause; GNOME matches the
  window (app_id `gh-release-updater`) through `StartupWMClass=gh-release-updater`.

## What changed
- `src-tauri/src/activation.rs` (new).
- `src-tauri/src/lib.rs`: save the token before the builder; single-instance
  callback uses it on the main thread before showing the window.

## Evidence
`WAYLAND_DEBUG=1` on the release build:
- Before: second launch with token `tok2` → primary only sent
  `xdg_activation_v1.activate("<fresh token>")`; `tok2` never appeared.
- After: primary launched with `tokA`, second with `tokB` → primary sent
  `activate("tokA_TIME0")` at its first map and `activate("tokB_TIME0")` on reopen;
  token file removed afterwards.
- Timing: first frame ~0.6 s after process start; second-instance handoff 0.22 s.
  The process itself is not slow to start.

## Corrections
- Initially suspected sync Tauri commands blocking the main thread or the desktop
  file not matching the window's app_id. The protocol log showed fast mapping and a
  matching app_id; the unused token was the actual problem.

## Left undone
- "Slow launch" was not reproduced at the process level (0.6 s to first frame from
  a terminal). If it persists after the fix, it is likely the same spinner, or a
  launch path not measured here (dock click, autostart).
- The installed `.deb` (`/usr/bin/gh-release-updater`) is an older build; the fix
  needs a rebuilt package installed.

## How to verify
1. `npm run tauri build`, install the new `.deb`.
2. Launch from the GNOME app grid, close the window (stays in tray), launch again
   from the grid/dock: the window comes forward and the cursor returns to normal.
3. Protocol check: run the binary with `WAYLAND_DEBUG=1 XDG_ACTIVATION_TOKEN=a_TIME0`,
   then a second one with `XDG_ACTIVATION_TOKEN=b_TIME0`; the first one's log must
   contain `activate("b_TIME0"`.
