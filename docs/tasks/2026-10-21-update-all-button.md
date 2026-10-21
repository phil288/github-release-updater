# Update all button

- **Date:** 2026-10-21
- **Branch:** `feat/initial-app`
- **Status:** done (not clicked live)

## Context
With several pending updates the user had to click Install on each card and wait for it to finish before the next one could start.

## Decisions
- "Update all (n)" button in the Updates page header, shown only when two or more updates are pending (with one, the card's Install button is the same action).
- Frontend loop in `Updates.svelte` calling the existing `install()` for each pending update, in order. No new backend command: `install_update` already serializes through `install_lock`, re-checks for a newer release and emits progress per app, so each card shows its own progress as before.
- The pending list is snapshotted at start; an app that dropped out of `store.status.pending` meanwhile (installed elsewhere, skipped) is passed over.
- A failed install moves on to the next app. A cancelled elevation prompt (password / UAC) stops the batch — the user declined, and re-prompting for every remaining app would be hostile.
- While the batch runs, Check now, per-card Install and Skip are disabled.

## Rejected alternatives
- Backend `install_all` command with a single elevation: would need one privileged process installing several packages; bigger change across Linux and Windows installers. Not worth it for now.
- Continuing after a cancel: would re-prompt for each remaining app.

## What changed
- `src/components/Updates.svelte`: `install()` returns its final stage; new `updateAll()`; header button; buttons disabled during the batch.

## Evidence
`npm run check`: 0 errors, 0 warnings.

## Left undone
- Each app still gets its own password/UAC prompt.
- No "stop after current" control during a batch.

## How to verify
1. Track two or more apps with older installed versions, Check now.
2. Click "Update all (n)": installs run one after another, each card shows progress.
3. Cancel one elevation prompt: remaining apps are not attempted.
