# Backblaze B2 backups

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done (unit-tested; not run against a real bucket)

## Context
Settings could only be backed up to a local JSON file. The user asked for a Backblaze
backup option in Settings, then (while the work was in progress) for a button that tests
the configuration and for a way to view every backup on Backblaze and restore any of them.

## Decisions
- **Native B2 API, v4** (`b2_authorize_account`, `b2_list_buckets`, `b2_get_upload_url`,
  `b2_upload_file`, `b2_list_file_names`, `b2_download_file_by_id`). Basic auth plus JSON
  over the existing `reqwest` stack; the only new crate is `sha1` (B2 requires
  `X-Bz-Content-Sha1` on upload).
- **One object per backup**: `<folder>/gh-release-updater-backup-<UTC YYYY-MM-DDTHH-MM-SSZ>.json`.
  This gives a history to list and restore that does not depend on the bucket's
  lifecycle/versioning rules. Listing uses `delimiter: "/"` and filters on the exact name
  pattern, so other files in the bucket are ignored. Results are sorted newest first.
- **Restore by file ID**: the list returns `fileId`, and restore downloads that ID, then runs the
  same `backup::parse` validation and the same merge/replace path as a file restore
  (`restore_settings` in `src-tauri/src/commands.rs`, shared by both).
- **B2 configuration is machine-local** (`b2.json` in the config directory), not part of
  `Settings`. Otherwise a merge or replace restore would overwrite or erase the very B2
  settings used to fetch the backup, and every backup would carry the key ID.
- **Application key** is stored in `b2-application-key` (mode 0600 on Unix), using the same
  helper as the GitHub token (`write_secret`/`read_secret`). It is never returned to the UI
  (only `has_key`) and never written to a backup.
- **Test connection** checks the *unsaved* form values (falling back to the stored key when the
  key field is blank). It authorizes, resolves the bucket, reports missing capabilities
  (`writeFiles`, `listFiles`, `readFiles`) and a key name-prefix restriction that excludes the
  folder, and counts existing backups. It writes nothing.
- **Automatic backup** (opt-in): `AppState::save_settings` signals a `Notify`. A background task
  waits 10 s to collapse bursts of saves, skips the upload when the settings equal the last
  uploaded ones, and shows a desktop notification only for the first failure in a row.
  Saving the B2 configuration with auto-backup on triggers one backup.
- "Back up now" and "View backups" are disabled while the form has unsaved changes, so they always
  use the saved configuration.

## Rejected alternatives
- **S3-compatible API**: needs SigV4 signing (another crate or hand-rolled HMAC) for no gain here.
- **Single fixed object name relying on B2 file versions**: simpler, but the history would depend
  on bucket lifecycle settings, and listing versions is less obvious to users.
- **Storing the B2 config inside `Settings`**: see above (restore would clobber it).
- **Deleting backups from the UI / retention policy**: not requested, and deletion is permanent.
  Left for later.

## What changed
- `crates/updater-core/src/b2.rs` (new): `B2Config`, `B2` client (`test`, `upload`, `list`,
  `download`), error mapping, name encoding, tests.
- `crates/updater-core/src/backup.rs`: `to_json` extracted from `export`.
- `crates/updater-core/src/error.rs`: `Error::Backblaze`.
- `crates/updater-core/Cargo.toml`: `sha1 = "0.10"`.
- `src-tauri/src/state.rs`: B2 paths, secret helpers, `b2` client, `settings_saved` signal.
- `src-tauri/src/commands.rs`: `get_b2_config`, `save_b2_config`, `clear_b2_config`, `test_b2`,
  `b2_backup_now`, `b2_list_backups`, `b2_restore`; shared `restore_settings`.
- `src-tauri/src/lib.rs`: command registration, `auto_backup` task.
- `src/components/BackblazeBackup.svelte` (new), mounted at the bottom of `Settings.svelte`;
  `src/lib/{api,types}.ts`.
- `README.md`: feature list.

## Evidence
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.
- `cargo test --workspace --locked`: 32 passed (6 new in `b2::tests`: prefix normalisation,
  config validation, object names and filtering, header encoding, v4 authorize parsing including
  `buckets: null`, error messages).
- `npm run check`: 0 errors, 0 warnings. `npm run build`: OK.
- **Not done:** no request was made against a real B2 account, and the UI was not clicked through
  in the running app.

## Corrections
- First draft targeted API **v3** and parsed `allowed.buckets`. The Backblaze docs now document only
  v4, and `allowed.buckets` (array of `{id, name}`, name nullable) exists only in v4; v3 used
  `allowed.bucketId`/`bucketName`. Switched every endpoint to `/b2api/v4/`. When a restricted
  bucket's name is null, the code falls back to `b2_list_buckets` by name.
- The draft sent `startFileName: null` on the first list page. The docs do not state that null is
  accepted, so the field is now omitted until there is a next page.

## Left undone
- Deleting backups, retention ("keep last N"), scheduled backups that do not depend on a settings change.
- Windows: the key file gets default ACLs (same as the GitHub token today).
- An integration test against a mock B2 server.

## How to verify
1. `cargo test --workspace` and `npm run check`.
2. In the B2 console, create a bucket and an application key restricted to it with
   `writeFiles`, `listFiles` and `readFiles`.
3. Run the app (`npm run tauri dev`). In Settings → Backblaze B2 backup, enter the key ID, key, bucket
   and an optional folder, then click **Test connection**. It should report the bucket and 0 backups.
   A wrong key should give "the key ID or application key was rejected".
4. **Save**, then **Back up now**, then **View backups**. The new file is listed with its date and size.
5. Change a setting, then **Merge**/**Replace** on the listed backup. The setting reverts.
6. Tick auto-backup and save. About 10 s later a new backup appears after **Refresh list**.
