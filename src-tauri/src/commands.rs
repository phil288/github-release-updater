//! Commands invoked from the web UI.

use crate::state::{self, AppState, Status};
use crate::{apply_autostart, emit_status, notify, run_check};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use updater_core::assets::Platform;
use updater_core::b2::{B2Config, RemoteBackup, TestReport};
use updater_core::backup::{self, ImportMode};
use updater_core::check::{self, CheckResult, InstallReport, Preview, Progress};
use updater_core::config::{Settings, TrackedApp};
use updater_core::installer::{self, Outcome};
use updater_core::{detect, version};
use uuid::Uuid;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let saved = state.save_settings(settings)?;
    apply_autostart(&app, saved.autostart)?;
    state.wake.notify_one();
    emit_status(&app);
    Ok(saved)
}

/// Creates a tracked app from user input and fills in what can be detected.
#[tauri::command]
pub async fn new_app(repo: String) -> CmdResult<TrackedApp> {
    let mut app = TrackedApp::new(&repo).map_err(err)?;
    if cfg!(target_os = "linux") {
        // Most projects name their Debian package after the repository.
        app.package_name = Some(app.display_name().to_ascii_lowercase());
    }
    app.installed_version = detect::installed_version(&app).await;
    if app.installed_version.is_none() && cfg!(target_os = "linux") {
        app.package_name = None;
    }
    Ok(app)
}

#[tauri::command]
pub async fn preview_app(state: State<'_, AppState>, app: TrackedApp) -> CmdResult<Option<Preview>> {
    check::preview(&state.client, &app, Platform::current()).await.map_err(err)
}

#[tauri::command]
pub async fn detect_installed(app: TrackedApp) -> Option<String> {
    detect::installed_version(&app).await
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Status {
    state.status()
}

#[tauri::command]
pub async fn check_now(app: AppHandle) -> Status {
    run_check(&app).await;
    app.state::<AppState>().status()
}

#[derive(Clone, serde::Serialize)]
struct ProgressEvent {
    app_id: Uuid,
    #[serde(flatten)]
    progress: Progress,
}

fn version_of_tag(tag: &str) -> String {
    version::parse_tag(tag).map(|v| v.to_string()).unwrap_or_else(|| tag.to_string())
}

/// Final result of an install, for the UI. Sent by every install, including
/// automatic ones the UI did not start.
#[derive(Clone, serde::Serialize)]
struct FinishedEvent {
    app_id: Uuid,
    stage: &'static str,
    message: String,
}

#[tauri::command]
pub async fn install_update(app: AppHandle, app_id: Uuid) -> CmdResult<InstallReport> {
    install_pending(&app, app_id, false).await
}

/// Installs the pending update of `app_id`. With `wait`, queues behind a
/// running installation instead of failing (automatic installs).
pub(crate) async fn install_pending(app: &AppHandle, app_id: Uuid, wait: bool) -> CmdResult<InstallReport> {
    let state = app.state::<AppState>();
    let _guard = if wait {
        state.install_lock.lock().await
    } else {
        state.install_lock.try_lock().map_err(|_| "Another installation is in progress.")?
    };
    let update = state
        .pending
        .lock()
        .unwrap()
        .get(&app_id)
        .cloned()
        .ok_or("This update is no longer pending; run a new check.")?;

    // Re-check right before installing: a newer release may have been published
    // since the last check, and updates always go straight to the highest version.
    let log = |line: String| {
        let _ = app.emit("install-progress", ProgressEvent { app_id, progress: Progress::Log { line } });
    };
    let tracked = state.settings().app(app_id).cloned().ok_or("Unknown app.")?;
    let update = match check::check_app(&state.client, &tracked, Platform::current()).await {
        CheckResult::Available(fresh) => {
            let fresh = *fresh;
            if fresh.tag != update.tag {
                log(format!("{} was published since the last check; installing it instead of {}.", fresh.tag, update.tag));
                state.pending.lock().unwrap().insert(app_id, fresh.clone());
            }
            fresh
        }
        CheckResult::Error { message } => {
            log(format!("Could not re-check for a newer release ({message}); installing {}.", update.tag));
            update
        }
        other => {
            // Installed elsewhere meanwhile, skipped, or the newest release lacks a package.
            let reason = match &other {
                CheckResult::UpToDate { tag } => format!("{} {tag} is already installed.", update.app_name),
                CheckResult::NoMatchingAsset { tag, .. } => {
                    format!("The newest release {tag} has no package for this system yet; try again later.")
                }
                _ => "This update is no longer available; run a new check.".to_string(),
            };
            state.pending.lock().unwrap().remove(&app_id);
            state.results.lock().unwrap().insert(app_id, other);
            emit_status(app);
            return Err(reason);
        }
    };
    if !update.intermediate_tags.is_empty() {
        log(format!(
            "Upgrading directly to {}; skipping {}.",
            update.tag,
            update.intermediate_tags.join(", ")
        ));
    }

    *state.installing.lock().unwrap() = Some(app_id);
    emit_status(app);

    let handle = app.clone();
    let last = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
    let progress = move |p: Progress| {
        // Throttle download progress; always send the final chunk.
        if let Progress::Downloading { downloaded, total } = &p {
            if Some(*downloaded) != *total {
                let mut l = last.lock().unwrap();
                if l.elapsed() < Duration::from_millis(150) {
                    return;
                }
                *l = Instant::now();
            }
        }
        let _ = handle.emit("install-progress", ProgressEvent { app_id, progress: p });
    };

    let settings = state.settings();
    let result = check::download_and_install(
        &state.client,
        &update,
        &settings,
        &state.paths.cache,
        &state.paths.logs,
        progress,
    )
    .await;
    *state.installing.lock().unwrap() = None;

    match &result {
        Ok(report) if matches!(report.outcome, Outcome::Installed | Outcome::InstalledRebootRequired) => {
            let mut s = state.settings();
            if let Some(a) = s.app_mut(app_id) {
                if a.package_name.is_none() {
                    a.package_name = report.package_name.clone();
                }
                let detected = detect::installed_version(a).await;
                a.installed_version = Some(detected.unwrap_or_else(|| version_of_tag(&update.tag)));
                a.skipped_version = None;
            }
            if let Err(e) = state.save_settings(s) {
                eprintln!("could not save settings after install: {e}");
            }
            state.pending.lock().unwrap().remove(&app_id);
            state.results.lock().unwrap().insert(app_id, CheckResult::UpToDate { tag: update.tag.clone() });
            let extra = if report.outcome == Outcome::InstalledRebootRequired { " A restart is required." } else { "" };
            notify(app, "Update installed", &format!("{} {} was installed.{extra}", update.app_name, update.tag));
        }
        Ok(report) => {
            if let Outcome::Failed { message, .. } = &report.outcome {
                notify(app, "Update failed", &format!("{} {}: {message}", update.app_name, update.tag));
            }
        }
        Err(e) => notify(app, "Update failed", &format!("{} {}: {e}", update.app_name, update.tag)),
    }
    let (stage, message) = match &result {
        Ok(r) => match &r.outcome {
            Outcome::Installed => ("done", format!("{} {} installed.", update.app_name, r.tag)),
            Outcome::InstalledRebootRequired => {
                ("done", format!("{} {} installed — restart required.", update.app_name, r.tag))
            }
            Outcome::Cancelled => ("cancelled", "Installation cancelled.".to_string()),
            Outcome::Failed { message, .. } => ("failed", message.clone()),
            Outcome::Uninstalled | Outcome::UninstalledRebootRequired => ("failed", "Unexpected outcome.".to_string()),
        },
        Err(e) => ("failed", e.to_string()),
    };
    let _ = app.emit("install-finished", FinishedEvent { app_id, stage, message });
    emit_status(app);
    result.map_err(err)
}

/// Uninstalls a tracked app through the package manager. On success the app is
/// disabled so the next check does not offer to reinstall it; the UI then asks
/// whether to stop tracking it altogether.
#[tauri::command]
pub async fn uninstall_app(app: AppHandle, app_id: Uuid) -> CmdResult<Outcome> {
    let state = app.state::<AppState>();
    let tracked = state.settings().app(app_id).cloned().ok_or("Unknown app.")?;
    let _guard = state.install_lock.try_lock().map_err(|_| "Another installation is in progress.")?;

    *state.installing.lock().unwrap() = Some(app_id);
    emit_status(&app);
    let handle = app.clone();
    let log = move |line: String| {
        let _ = handle.emit("install-progress", ProgressEvent { app_id, progress: Progress::Log { line } });
    };
    let settings = state.settings();
    let opts = installer::Options { msi_ui: settings.msi_ui, log_dir: &state.paths.logs };
    let result = installer::uninstall(&tracked, opts, log).await;
    *state.installing.lock().unwrap() = None;

    let name = tracked.display_name();
    match &result {
        Ok(Outcome::Uninstalled | Outcome::UninstalledRebootRequired) => {
            let mut s = state.settings();
            if let Some(a) = s.app_mut(app_id) {
                a.installed_version = detect::installed_version(a).await;
                a.skipped_version = None;
                a.enabled = false;
            }
            if let Err(e) = state.save_settings(s) {
                eprintln!("could not save settings after uninstall: {e}");
            }
            state.pending.lock().unwrap().remove(&app_id);
            state.results.lock().unwrap().remove(&app_id);
            let extra = if matches!(result, Ok(Outcome::UninstalledRebootRequired)) { " A restart is required." } else { "" };
            notify(&app, "Application uninstalled", &format!("{name} was uninstalled.{extra}"));
        }
        Ok(Outcome::Failed { message, .. }) => notify(&app, "Uninstall failed", &format!("{name}: {message}")),
        Ok(_) => {}
        Err(e) => notify(&app, "Uninstall failed", &format!("{name}: {e}")),
    }
    emit_status(&app);
    result.map_err(err)
}

#[tauri::command]
pub fn skip_version(app: AppHandle, state: State<'_, AppState>, app_id: Uuid) -> CmdResult<()> {
    let tag = state
        .pending
        .lock()
        .unwrap()
        .get(&app_id)
        .map(|u| u.tag.clone())
        .ok_or("No pending update for this app.")?;
    let mut s = state.settings();
    s.app_mut(app_id).ok_or("Unknown app.")?.skipped_version = Some(tag.clone());
    state.save_settings(s)?;
    state.pending.lock().unwrap().remove(&app_id);
    state.results.lock().unwrap().insert(app_id, CheckResult::Skipped { tag });
    emit_status(&app);
    Ok(())
}

/// Asks for a destination and writes the backup. Returns the path, or `None` if cancelled.
#[tauri::command]
pub async fn export_backup(app: AppHandle) -> CmdResult<Option<String>> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Back up settings")
        .add_filter("Backup", &["json"])
        .set_file_name(backup::default_file_name())
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(err)?;
    let settings = app.state::<AppState>().settings();
    backup::export(&settings, &path).map_err(err)?;
    Ok(Some(path.display().to_string()))
}

/// Asks for a backup file and applies it. Returns the new settings, or `None` if cancelled.
#[tauri::command]
pub async fn import_backup(app: AppHandle, mode: ImportMode) -> CmdResult<Option<Settings>> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Restore settings")
        .add_filter("Backup", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(err)?;
    let imported = backup::read(&path).map_err(err)?;
    restore_settings(&app, imported, mode).await.map(Some)
}

/// Applies imported settings (from a file or from B2) and resets check state.
async fn restore_settings(app: &AppHandle, imported: Settings, mode: ImportMode) -> CmdResult<Settings> {
    let state = app.state::<AppState>();
    let mut merged = backup::apply(&state.settings(), imported, mode).map_err(err)?;
    // A backup may come from another machine: trust what is actually installed here.
    for a in &mut merged.apps {
        if let Some(v) = detect::installed_version(a).await {
            a.installed_version = Some(v);
        }
    }
    let saved = state.save_settings(merged)?;
    apply_autostart(app, saved.autostart)?;
    state.pending.lock().unwrap().clear();
    state.results.lock().unwrap().clear();
    state.wake.notify_one();
    emit_status(app);
    Ok(saved)
}

#[derive(serde::Serialize)]
pub struct B2State {
    config: B2Config,
    has_key: bool,
}

#[tauri::command]
pub fn get_b2_config(state: State<'_, AppState>) -> B2State {
    B2State { config: state::read_b2_config(&state.paths), has_key: state::read_b2_key(&state.paths).is_some() }
}

/// Saves the B2 configuration. `app_key` replaces the stored key when given;
/// `None` or blank keeps the current one.
#[tauri::command]
pub fn save_b2_config(state: State<'_, AppState>, config: B2Config, app_key: Option<String>) -> CmdResult<B2State> {
    let config = config.validate().map_err(err)?;
    if let Some(key) = app_key.as_deref().filter(|k| !k.trim().is_empty()) {
        state::write_b2_key(&state.paths, Some(key)).map_err(err)?;
    }
    config.save(&state.paths.b2_config).map_err(err)?;
    if config.auto_backup {
        // Back up the current settings soon rather than at the next change.
        state.settings_saved.notify_one();
    }
    Ok(get_b2_config(state))
}

/// Forgets the B2 configuration and deletes the stored application key.
/// Backups already in the bucket are left alone.
#[tauri::command]
pub fn clear_b2_config(state: State<'_, AppState>) -> CmdResult<B2State> {
    state::write_b2_key(&state.paths, None).map_err(err)?;
    match std::fs::remove_file(&state.paths.b2_config) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(err(e)),
        _ => {}
    }
    Ok(get_b2_config(state))
}

fn b2_key(state: &AppState, typed: Option<String>) -> CmdResult<String> {
    typed
        .filter(|k| !k.trim().is_empty())
        .or_else(|| state::read_b2_key(&state.paths))
        .ok_or_else(|| "Enter the application key first.".to_string())
}

/// Tests the given (possibly unsaved) configuration. Uploads nothing.
#[tauri::command]
pub async fn test_b2(state: State<'_, AppState>, config: B2Config, app_key: Option<String>) -> CmdResult<TestReport> {
    let config = config.validate().map_err(err)?;
    let key = b2_key(&state, app_key)?;
    state.b2.test(&config, &key).await.map_err(err)
}

/// Uploads a backup now and returns the object name.
#[tauri::command]
pub async fn b2_backup_now(state: State<'_, AppState>) -> CmdResult<String> {
    let key = b2_key(&state, None)?;
    let config = state::read_b2_config(&state.paths);
    state.b2.upload(&config, &key, &state.settings()).await.map_err(err)
}

#[tauri::command]
pub async fn b2_list_backups(state: State<'_, AppState>) -> CmdResult<Vec<RemoteBackup>> {
    let key = b2_key(&state, None)?;
    state.b2.list(&state::read_b2_config(&state.paths), &key).await.map_err(err)
}

#[tauri::command]
pub async fn b2_restore(app: AppHandle, file_id: String, mode: ImportMode) -> CmdResult<Settings> {
    let state = app.state::<AppState>();
    let key = b2_key(&state, None)?;
    let imported = state.b2.download(&state::read_b2_config(&state.paths), &key, &file_id).await.map_err(err)?;
    restore_settings(&app, imported, mode).await
}

#[tauri::command]
pub fn has_token(state: State<'_, AppState>) -> bool {
    state::read_token(&state.paths).is_some()
}

#[tauri::command]
pub fn set_token(state: State<'_, AppState>, token: Option<String>) -> CmdResult<()> {
    state::write_token(&state.paths, token.as_deref()).map_err(err)?;
    state.client.set_token(token);
    state.wake.notify_one();
    Ok(())
}

#[tauri::command]
pub fn open_release_page(app: AppHandle, url: String) -> CmdResult<()> {
    if !url.starts_with("https://github.com/") {
        return Err("Only github.com links can be opened.".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}

#[tauri::command]
pub fn platform() -> Platform {
    Platform::current()
}
