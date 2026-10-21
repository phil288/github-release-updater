//! Shared application state and file locations.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::menu::MenuItem;
use tauri::{Manager, Wry};
use updater_core::check::{CheckResult, UpdateInfo};
use updater_core::b2::{B2Config, B2};
use updater_core::config::Settings;
use updater_core::github::Client;
use uuid::Uuid;

pub struct Paths {
    pub config: PathBuf,
    pub token: PathBuf,
    pub b2_config: PathBuf,
    pub b2_key: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
}

impl Paths {
    pub fn resolve(app: &tauri::AppHandle) -> tauri::Result<Self> {
        let p = app.path();
        let config_dir = p.app_config_dir()?;
        Ok(Self {
            config: config_dir.join("config.json"),
            token: config_dir.join("github-token"),
            b2_config: config_dir.join("b2.json"),
            b2_key: config_dir.join("b2-application-key"),
            cache: p.app_cache_dir()?.join("downloads"),
            logs: p.app_log_dir()?,
        })
    }
}

pub struct AppState {
    pub paths: Paths,
    pub settings: Mutex<Settings>,
    pub client: Client,
    pub b2: B2,
    pub pending: Mutex<BTreeMap<Uuid, UpdateInfo>>,
    pub results: Mutex<HashMap<Uuid, CheckResult>>,
    pub last_check: Mutex<Option<u64>>,
    pub checking: AtomicBool,
    pub installing: Mutex<Option<Uuid>>,
    /// Serialises installs: package managers do not like concurrent runs.
    pub install_lock: tokio::sync::Mutex<()>,
    /// Wakes the scheduler early (settings changed).
    pub wake: tokio::sync::Notify,
    /// Signals the automatic B2 backup task that the settings were saved.
    pub settings_saved: tokio::sync::Notify,
    pub tray_updates_item: Mutex<Option<MenuItem<Wry>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub pending: Vec<UpdateInfo>,
    pub results: HashMap<Uuid, CheckResult>,
    /// Unix seconds of the last completed check.
    pub last_check: Option<u64>,
    pub checking: bool,
    pub installing: Option<Uuid>,
}

impl AppState {
    pub fn status(&self) -> Status {
        Status {
            pending: self.pending.lock().unwrap().values().cloned().collect(),
            results: self.results.lock().unwrap().clone(),
            last_check: *self.last_check.lock().unwrap(),
            checking: self.checking.load(std::sync::atomic::Ordering::SeqCst),
            installing: *self.installing.lock().unwrap(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_settings(&self, settings: Settings) -> Result<Settings, String> {
        let settings = settings.validate().map_err(|e| e.to_string())?;
        settings.save(&self.paths.config).map_err(|e| e.to_string())?;
        // Forget pending updates and results of apps that were removed or disabled.
        let keep: std::collections::HashSet<Uuid> =
            settings.apps.iter().filter(|a| a.enabled).map(|a| a.id).collect();
        self.pending.lock().unwrap().retain(|id, _| keep.contains(id));
        self.results.lock().unwrap().retain(|id, _| keep.contains(id));
        *self.settings.lock().unwrap() = settings.clone();
        self.settings_saved.notify_one();
        Ok(settings)
    }
}

/// The GitHub token lives in its own file (mode 0600), never in config.json,
/// so backups cannot leak it.
pub fn read_token(paths: &Paths) -> Option<String> {
    read_secret(&paths.token)
}

pub fn write_token(paths: &Paths, token: Option<&str>) -> std::io::Result<()> {
    write_secret(&paths.token, token)
}

/// The B2 application key is stored like the GitHub token.
pub fn read_b2_key(paths: &Paths) -> Option<String> {
    read_secret(&paths.b2_key)
}

pub fn write_b2_key(paths: &Paths, key: Option<&str>) -> std::io::Result<()> {
    write_secret(&paths.b2_key, key)
}

pub fn read_b2_config(paths: &Paths) -> B2Config {
    B2Config::load(&paths.b2_config).unwrap_or_else(|e| {
        eprintln!("could not load {}: {e}", paths.b2_config.display());
        B2Config::default()
    })
}

fn read_secret(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

fn write_secret(path: &Path, secret: Option<&str>) -> std::io::Result<()> {
    match secret.map(str::trim).filter(|t| !t.is_empty()) {
        None => match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
        Some(t) => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            std::io::Write::write_all(&mut opts.open(path)?, t.as_bytes())
        }
    }
}
