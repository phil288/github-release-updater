//! Export and import of settings. Backups never contain the GitHub token.

use crate::config::{write_atomic, Settings};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const FORMAT: &str = "gh-release-updater-backup";

#[derive(Debug, Serialize, Deserialize)]
struct BackupFile {
    format: String,
    exported_at: String,
    app_version: String,
    settings: Settings,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImportMode {
    /// Imported settings replace the current ones entirely.
    Replace,
    /// Global preferences come from the backup; apps are merged by repository.
    /// Apps already tracked keep their machine-local installed/skipped versions.
    Merge,
}

/// Suggested file name, e.g. `gh-release-updater-backup-2026-10-20.json`.
pub fn default_file_name() -> String {
    format!(
        "gh-release-updater-backup-{}.json",
        chrono::Local::now().format("%Y-%m-%d")
    )
}

pub fn export(settings: &Settings, path: &Path) -> Result<()> {
    write_atomic(path, to_json(settings)?.as_bytes())
}

/// The backup document written by [`export`] and uploaded to Backblaze B2.
pub fn to_json(settings: &Settings) -> Result<String> {
    let file = BackupFile {
        format: FORMAT.into(),
        exported_at: chrono::Local::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        settings: settings.clone(),
    };
    Ok(serde_json::to_string_pretty(&file)?)
}

/// Reads and validates a backup. A bare settings file (config.json) is accepted too.
pub fn read(path: &Path) -> Result<Settings> {
    let text = std::fs::read_to_string(path)?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<Settings> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| Error::InvalidBackup(format!("not JSON: {e}")))?;
    let settings_value = match value.get("format").and_then(|f| f.as_str()) {
        Some(FORMAT) => value
            .get("settings")
            .cloned()
            .ok_or_else(|| Error::InvalidBackup("missing settings".into()))?,
        Some(other) => return Err(Error::InvalidBackup(format!("unknown format '{other}'"))),
        None if value.get("apps").is_some() => value,
        None => {
            return Err(Error::InvalidBackup(
                "not a GitHub Release Updater backup".into(),
            ))
        }
    };
    let settings: Settings =
        serde_json::from_value(settings_value).map_err(|e| Error::InvalidBackup(e.to_string()))?;
    settings.validate()
}

pub fn apply(current: &Settings, imported: Settings, mode: ImportMode) -> Result<Settings> {
    let merged = match mode {
        ImportMode::Replace => imported,
        ImportMode::Merge => {
            let mut out = Settings {
                apps: current.apps.clone(),
                ..imported.clone()
            };
            for app in imported.apps {
                match out
                    .apps
                    .iter_mut()
                    .find(|a| a.repo.eq_ignore_ascii_case(&app.repo))
                {
                    Some(existing) => {
                        let (id, installed, skipped) = (
                            existing.id,
                            existing.installed_version.clone(),
                            existing.skipped_version.clone(),
                        );
                        *existing = app;
                        existing.id = id;
                        existing.installed_version = installed;
                        existing.skipped_version = skipped;
                    }
                    None => out.apps.push(app),
                }
            }
            out
        }
    };
    merged.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TrackedApp;

    fn app(repo: &str, installed: Option<&str>) -> TrackedApp {
        let mut a = TrackedApp::new(repo).unwrap();
        a.installed_version = installed.map(Into::into);
        a
    }

    #[test]
    fn export_then_read() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("backup.json");
        let mut s = Settings {
            check_interval_hours: 3,
            ..Settings::default()
        };
        s.apps.push(app("storytold/wordcraft", Some("0.7.0")));
        export(&s, &p).unwrap();
        assert_eq!(read(&p).unwrap(), s);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("nope").is_err());
        assert!(parse(r#"{"hello":1}"#).is_err());
        assert!(parse(r#"{"format":"other","settings":{}}"#).is_err());
        assert!(parse(
            r#"{"format":"gh-release-updater-backup","settings":{"apps":[{"repo":"bad"}]}}"#
        )
        .is_err());
        assert!(
            parse(r#"{"apps":[{"repo":"a/b"}]}"#).is_ok(),
            "bare config accepted"
        );
    }

    #[test]
    fn merge_keeps_local_state() {
        let mut current = Settings::default();
        current.apps.push(app("a/one", Some("1.0.0")));
        current.apps.push(app("a/two", None));
        let mut imported = Settings {
            check_interval_hours: 12,
            ..Settings::default()
        };
        let mut one = app("A/ONE", Some("0.1.0"));
        one.include_prerelease = true;
        imported.apps.push(one);
        imported.apps.push(app("a/three", Some("3.0.0")));

        let merged = apply(&current, imported.clone(), ImportMode::Merge).unwrap();
        assert_eq!(merged.check_interval_hours, 12);
        assert_eq!(merged.apps.len(), 3);
        assert_eq!(merged.apps[0].id, current.apps[0].id);
        assert_eq!(merged.apps[0].installed_version.as_deref(), Some("1.0.0"));
        assert!(merged.apps[0].include_prerelease);

        let replaced = apply(&current, imported, ImportMode::Replace).unwrap();
        assert_eq!(replaced.apps.len(), 2);
        assert_eq!(replaced.apps[0].installed_version.as_deref(), Some("0.1.0"));
    }
}
