# Upgrade straight to the latest version

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done

## Context
Requirement: an upgrade must go directly to the newest release and skip every version in
between. The first implementation already offered only one release. Three gaps could still
install an older or intermediate version:

1. `GET /releases/latest` returns the most recently *published* release, not the highest version.
   A backport (v0.7.5 published after v0.8.0) would have been offered instead of v0.8.0.
2. The pending update was frozen at check time (up to `check_interval_hours` old). Clicking Install
   after a newer release had appeared installed the older one.
3. "Skip this version" matched only the exact tag.

## Decisions
- Always read `GET /releases?per_page=50` and choose the **highest version** among eligible
  releases (`github::pick_highest`). Eligibility: never drafts; pre-releases only with
  `include_prerelease`. A semver pre-release tag (`v1.0.0-rc.1`) counts as a pre-release even
  when GitHub's flag is missing.
- **Re-check at install time** (`install_update`): if a higher release exists, it replaces the
  pending one and the progress log says so. If the app is up to date now, the newest release has
  no package for this system, or the release was skipped, the install is refused with a reason.
  If the re-check itself fails (offline, rate limit), the pending release is installed and the
  failure is logged.
- **Skip covers lower versions too**: a release is skipped while `tag <= skipped_version`. A
  higher release is offered again.
- `UpdateInfo.intermediate_tags` lists the releases that are jumped over. The Updates card shows
  them, and the installer log repeats them. `InstallReport.tag` reports the release that was
  actually installed.
- No fallback to an older release when the newest one lacks a package for this platform. It is
  reported as "no matching asset" and picked up on a later check, once assets have been uploaded.

## Rejected alternatives
- Falling back to the newest release that *has* a matching asset: this would install an
  intermediate version, which contradicts the requirement.
- Paging through all releases: 50 recent releases are enough to find the highest one. Only the
  intermediate list could be truncated, and it is informational.

## What changed
`crates/updater-core/src/github.rs` (`releases`, `eligible`, `pick_highest`; `latest_release` removed),
`crates/updater-core/src/check.rs` (`evaluate` over the release list, `intermediate_tags`, skip ≤,
`InstallReport.tag`), `src-tauri/src/commands.rs` (re-check before install), `src/lib/types.ts`,
`src/components/Updates.svelte`.

## Evidence
- `cargo test -p updater-core`: 25 passed. New tests: the highest version beats a later-published
  backport; drafts and pre-releases are excluded; a semver pre-release without the flag is treated
  as a pre-release; the intermediate list is `[v0.8.0, v0.7.5, v0.6.0]` for 0.5.0 → 0.9.0; skipping
  v0.9.0 hides it and lower versions but not v0.10.0.
- Live probe against `storytold/wordcraft` still resolves v0.8.0 and the x86_64 `.deb`.
- clippy clean on Linux and the Windows target; `npm run check` 0 errors.

## Corrections
- The initial-app record said "GitHub's latest release" was used when pre-releases are off. That is
  replaced here by highest-version selection in both modes.

## Left undone
- Nothing for this requirement.

## How to verify
`cargo test -p updater-core check::tests::jumps_straight_to_highest github::tests`
