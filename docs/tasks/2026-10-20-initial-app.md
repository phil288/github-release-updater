# Initial application

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done on Linux (release `.deb` built and installed); Windows untested

## Context
Several GitHub projects (e.g. `storytold/wordcraft`) are installed by hand from their release
`.deb` (Ubuntu) and `.msi` (Windows) assets. Wanted: one app on both OSes that notices new
releases, asks, and installs them in the foreground with the OS's own elevation prompt
(polkit password / UAC). Configured through a settings UI with backup and restore.

## Decisions
- **Tauri 2 + Svelte 5** (user choice over Python/PySide6 and .NET/Avalonia). Tauri 3 is alpha, so 2.x.
- **Workspace split**: `crates/updater-core` holds every piece of logic with no GUI dependency, so
  it builds and tests without WebKitGTK and cross-checks for Windows (`--target x86_64-pc-windows-gnu`).
  `src-tauri` is thin glue.
- **Tray app, autostart with `--minimized`**; closing the window hides it. Autostart is not touched
  in debug builds so `tauri dev` does not register the dev binary as a login item.
- **Approval lives in the window, not the toast.** Tauri's desktop notification API has no
  click/action callbacks, so a notification announces the update and (setting
  `open_window_on_update`, default on) the Updates page opens. Tray menu shows the pending count.
- **Ubuntu install**: copy the `.deb` into a fresh `/tmp/gh-release-updater-*` dir (0755/0644) so
  `_apt` can read it and root does not install from a user-writable path, then
  `pkexec env DEBIAN_FRONTEND=noninteractive apt-get install -y <file>`. apt resolves dependencies,
  unlike `dpkg -i`. pkexec exit 126 → cancelled, 127 → auth failed / no polkit agent.
- **Windows install**: `ShellExecuteExW` verb `runas` on `msiexec /i … /passive /norestart /l*v log`
  (UI level configurable: passive or full wizard). 1223 (UAC declined) and 1602 → cancelled;
  3010 → installed, reboot required.
- **Asset selection**: extension filter (.deb/.msi) → drop other architectures → explicit arch
  match (10) beats arch-less (1) → `cli/web/portable/debug/src` penalised (−5) → shortest name.
  `x86_64` is tested before `x86`. Per-OS regex override per app, so one backup works on both OSes.
- **Verification**: GitHub API `digest` (`sha256:…`) first, then `<asset>.sha256[sum]`, then a
  `SHA256SUMS*`/`*checksum*` list (GNU and BSD formats). Mismatch deletes the file and aborts.
  `require_checksum` (default off) refuses releases without one.
- **Download hardening**: HTTPS only, hosts allow-listed (`github.com`,
  `objects/release-assets/github-releases.githubusercontent.com`), redirects checked against the
  same list, file names sanitised, `.part` file renamed only when the byte count matches.
- **Version comparison**: lenient semver from tags (`v0.8.0`, `release-1.2`, `1.2.3.4`, `_rc1`);
  installed versions normalised (Debian epoch/revision, Windows 4-part) before comparing.
- **Installed version**: recorded after a successful install (re-read from dpkg/registry when
  possible); `Detect` button and auto-detect on add/restore. `package_name` is filled from the
  `.deb` control file after the first install.
- **Rate limits**: ETag cache (304s are free); optional token raises 60 → 5000 req/h.
- **Token storage**: separate file `github-token` (0600 on Unix) beside config.json, never in
  backups.
- **Backup**: wrapped JSON (`format`, `exported_at`, `app_version`, `settings`); a bare
  `config.json` is accepted too. Merge keeps machine-local `installed_version`/`skipped_version`
  of already-tracked apps; after any restore installed versions are re-detected.
- **TLS**: rustls on Linux, native-tls (SChannel) on Windows — uses the Windows cert store and
  avoids `ring`'s C build when cross-checking from Linux.

## Rejected alternatives
- `keyring` crate for the token: needs libdbus/secret-service on Linux; a 0600 file excluded
  from backups is enough for a token with no scopes.
- Auto-install without asking, portable `.zip` and `.exe` setups (out of scope per user).
- `dpkg -i`: does not resolve dependencies.
- Notification action buttons: not supported by Tauri's notification plugin on desktop.
- Running `apt-get` from the user's cache dir: `_apt` sandbox warning and root reading a
  user-writable file.

## What changed
New project: `Cargo.toml` (workspace), `crates/updater-core/*`, `src-tauri/*`, `src/*` (UI),
`assets/icon.svg` + generated `src-tauri/icons`, `.github/workflows/ci.yml` (originally `build.yml`), `README.md`.

## Evidence
- `cargo test -p updater-core`: 23 passed (repo parsing, version compare, asset selection on the
  real wordcraft v0.8.0 asset list for 6 OS/arch combos, checksum parsing, config roundtrip,
  backup export/import/merge, pipeline evaluation).
- `cargo clippy -p updater-core --all-targets` on Linux and `--target x86_64-pc-windows-gnu`:
  0 warnings (Windows installer + registry code type-checks).
- Live probe `cargo run -p updater-core --example probe -- …/wordcraft/releases/tag/v0.8.0 --download`:
  picked `wordcraft-0.8.0-linux-x86_64.deb`, downloaded 100,947,572 bytes through the redirect
  allow-list, SHA-256 matched the API digest.
- `npm run build` and `npm run check`: 0 errors, 0 warnings.

## Corrections
- The `src-tauri` crate compiled with zero clippy warnings once WebKitGTK was installed, and
  `tauri build --bundles deb` produced a working package. Its autostart entry was created on first launch.
- The Debian package name is `git-hub-release-updater` (Tauri kebab-cases "GitHub"), not
  `gh-release-updater`. The binary is `/usr/bin/gh-release-updater`.

## Left undone
- No real install run (pkexec / UAC) yet; Windows flow untested on a Windows machine.
- No self-update of this app beyond tracking its own repository.

## How to verify
1. `cargo test --workspace && npm run check`
2. `npm run tauri dev` → Applications → add `https://github.com/storytold/wordcraft/releases/tag/v0.8.0`
   → Test match shows the `.deb` → set installed version `0.7.0` → Updates → Check now →
   notification + Install → pkexec dialog → apt output → `dpkg -s wordcraft` reports 0.8.0.
3. Cancel the pkexec dialog → "Installation cancelled", update stays pending.
4. Settings → Back up… → remove an app → Restore and merge… → app is back.
5. Windows: same flow with UAC prompt and MSI progress window.
