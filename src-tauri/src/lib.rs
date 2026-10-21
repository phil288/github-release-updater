mod activation;
mod commands;
mod state;

use state::{AppState, Paths};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use updater_core::assets::Platform;
use updater_core::check::{self, CheckResult};
use updater_core::config::Settings;
use updater_core::b2::B2;
use updater_core::github::Client;
use updater_core::installer::Outcome;
use uuid::Uuid;

const STARTUP_DELAY: Duration = Duration::from_secs(20);
const AUTO_BACKUP_DELAY: Duration = Duration::from_secs(10);

/// Shows the main window and optionally switches it to a page.
pub(crate) fn show_window(app: &AppHandle, page: Option<&str>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(page) = page {
        let _ = app.emit("navigate", page);
    }
}

pub(crate) fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("notification failed: {e}");
    }
}

pub(crate) fn emit_status(app: &AppHandle) {
    let state = app.state::<AppState>();
    let status = state.status();
    let n = status.pending.len();
    if let Some(item) = state.tray_updates_item.lock().unwrap().as_ref() {
        let text = match n {
            0 => "No updates".to_string(),
            1 => "1 update available…".to_string(),
            n => format!("{n} updates available…"),
        };
        let _ = item.set_text(text);
    }
    if let Some(tray) = app.tray_by_id("main") {
        let tip = if n == 0 { "GitHub Release Updater".to_string() } else { format!("GitHub Release Updater — {n} update(s)") };
        let _ = tray.set_tooltip(Some(tip));
    }
    let _ = app.emit("status-changed", status);
}

/// Checks every enabled app and refreshes the pending list. Notifies about
/// updates that were not pending before.
pub(crate) async fn run_check(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.checking.swap(true, Ordering::SeqCst) {
        return;
    }
    emit_status(app);
    let settings = state.settings();
    let platform = Platform::current();
    let mut results = HashMap::new();
    let mut available = BTreeMap::new();
    for tracked in settings.apps.iter().filter(|a| a.enabled) {
        let result = check::check_app(&state.client, tracked, platform).await;
        if let CheckResult::Available(info) = &result {
            available.insert(tracked.id, (**info).clone());
        }
        results.insert(tracked.id, result);
    }

    let auto: Vec<Uuid> = if settings.auto_update {
        let declined = state.auto_declined.lock().unwrap();
        available
            .values()
            .filter(|u| settings.app(u.app_id).is_some_and(|a| a.auto_update))
            .filter(|u| declined.get(&u.app_id) != Some(&u.tag))
            .map(|u| u.app_id)
            .collect()
    } else {
        Vec::new()
    };
    let newly_found: Vec<String> = {
        let mut pending = state.pending.lock().unwrap();
        let fresh = available
            .values()
            .filter(|u| !auto.contains(&u.app_id))
            .filter(|u| pending.get(&u.app_id).is_none_or(|p| p.tag != u.tag))
            .map(|u| format!("{} {}", u.app_name, u.tag))
            .collect();
        *pending = available;
        fresh
    };
    *state.results.lock().unwrap() = results;
    *state.last_check.lock().unwrap() = Some(now_unix());
    state.checking.store(false, Ordering::SeqCst);
    emit_status(app);

    if !auto.is_empty() {
        // Spawned: `check_now` returns to the UI without waiting for installs.
        let app = app.clone();
        tauri::async_runtime::spawn(async move { auto_install(&app, auto).await });
    }

    if !newly_found.is_empty() {
        let title = if newly_found.len() == 1 { "Update available" } else { "Updates available" };
        notify(app, title, &format!("{}\nOpen GitHub Release Updater to install.", newly_found.join(", ")));
        if settings.open_window_on_update {
            show_window(app, Some("updates"));
        }
    }
}

/// Installs the given pending updates one after another, queuing behind any
/// manual install. A cancelled or failed install is remembered by tag and not
/// retried automatically, so a dismissed password / UAC prompt does not come
/// back at every check.
async fn auto_install(app: &AppHandle, ids: Vec<Uuid>) {
    let state = app.state::<AppState>();
    if state.auto_installing.swap(true, Ordering::SeqCst) {
        return;
    }
    for id in ids {
        let Some(tag) = state.pending.lock().unwrap().get(&id).map(|u| u.tag.clone()) else { continue };
        let declined = match commands::install_pending(app, id, true).await {
            Ok(report) => !matches!(report.outcome, Outcome::Installed | Outcome::InstalledRebootRequired),
            // Installed or skipped meanwhile: nothing to remember.
            Err(_) if !state.pending.lock().unwrap().contains_key(&id) => false,
            Err(e) => {
                eprintln!("automatic install of {tag} failed: {e}");
                true
            }
        };
        if declined {
            // Remember the tag actually attempted, which may be newer than `tag`.
            let tag = state.pending.lock().unwrap().get(&id).map_or(tag, |u| u.tag.clone());
            state.auto_declined.lock().unwrap().insert(id, tag);
        }
    }
    state.auto_installing.store(false, Ordering::SeqCst);
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

async fn scheduler(app: AppHandle) {
    let state = app.state::<AppState>();
    tokio::select! {
        _ = tokio::time::sleep(STARTUP_DELAY) => {}
        _ = state.wake.notified() => {}
    }
    loop {
        run_check(&app).await;
        let hours = state.settings().check_interval_hours.max(1) as u64;
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(hours * 3600)) => {}
            _ = state.wake.notified() => {}
        }
    }
}

/// Uploads a B2 backup shortly after the settings change, when automatic
/// backups are on. Bursts of saves are collapsed, unchanged settings are not
/// uploaded twice, and only the first failure of a streak is notified.
async fn auto_backup(app: AppHandle) {
    let state = app.state::<AppState>();
    let mut last_uploaded: Option<Settings> = None;
    let mut failing = false;
    loop {
        state.settings_saved.notified().await;
        tokio::time::sleep(AUTO_BACKUP_DELAY).await;
        let config = state::read_b2_config(&state.paths);
        let Some(key) = state::read_b2_key(&state.paths) else { continue };
        if !config.auto_backup || !config.is_configured() {
            continue;
        }
        let settings = state.settings();
        if last_uploaded.as_ref() == Some(&settings) {
            continue;
        }
        match state.b2.upload(&config, &key, &settings).await {
            Ok(_) => {
                last_uploaded = Some(settings);
                failing = false;
            }
            Err(e) => {
                eprintln!("automatic B2 backup failed: {e}");
                if !failing {
                    notify(&app, "Backup failed", &format!("Automatic backup to Backblaze B2 failed: {e}"));
                }
                failing = true;
            }
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let updates = MenuItem::with_id(app, "updates", "No updates", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
    let check = MenuItem::with_id(app, "check", "Check now", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&updates, &sep, &open, &check, &settings, &sep2, &quit])?;
    *app.state::<AppState>().tray_updates_item.lock().unwrap() = Some(updates);

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("GitHub Release Updater")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "updates" => show_window(app, Some("updates")),
            "open" => show_window(app, None),
            "settings" => show_window(app, Some("settings")),
            "check" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { run_check(&app).await });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
            | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                show_window(tray.app_handle(), None);
            }
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Registers or removes the login item. Skipped in debug builds so `tauri dev`
/// does not register the development binary.
pub(crate) fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let current = launcher.is_enabled().unwrap_or(false);
    let r = match (enabled, current) {
        (true, false) => launcher.enable(),
        (false, true) => launcher.disable(),
        _ => Ok(()),
    };
    r.map_err(|e| format!("autostart: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let minimized = std::env::args().any(|a| a == "--minimized");
    // Before single-instance runs: if another instance is already up, this
    // process exits during plugin setup and the token must already be saved.
    activation::save_launch_token();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                activation::use_forwarded_token();
                show_window(&handle, None);
            });
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            let paths = Paths::resolve(&handle)?;
            let settings = Settings::load(&paths.config).unwrap_or_else(|e| {
                eprintln!("could not load {}: {e}; starting with defaults", paths.config.display());
                Settings::default()
            });
            let client = Client::new(state::read_token(&paths))?;
            app.manage(AppState {
                paths,
                settings: Mutex::new(settings.clone()),
                client,
                b2: B2::new()?,
                pending: Default::default(),
                results: Default::default(),
                last_check: Default::default(),
                checking: Default::default(),
                installing: Default::default(),
                auto_installing: Default::default(),
                auto_declined: Default::default(),
                install_lock: Default::default(),
                wake: Default::default(),
                settings_saved: Default::default(),
                tray_updates_item: Default::default(),
            });
            build_tray(&handle)?;
            if let Err(e) = apply_autostart(&handle, settings.autostart) {
                eprintln!("{e}");
            }
            if !minimized {
                show_window(&handle, None);
            }
            tauri::async_runtime::spawn(scheduler(handle.clone()));
            tauri::async_runtime::spawn(auto_backup(handle));
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the app running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::new_app,
            commands::preview_app,
            commands::detect_installed,
            commands::get_status,
            commands::check_now,
            commands::install_update,
            commands::skip_version,
            commands::uninstall_app,
            commands::export_backup,
            commands::import_backup,
            commands::get_b2_config,
            commands::save_b2_config,
            commands::clear_b2_config,
            commands::test_b2,
            commands::b2_backup_now,
            commands::b2_list_backups,
            commands::b2_restore,
            commands::has_token,
            commands::set_token,
            commands::open_release_page,
            commands::platform,
        ])
        .run(tauri::generate_context!())
        .expect("error while running GitHub Release Updater");
}
