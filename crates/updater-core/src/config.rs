//! Persistent settings: tracked applications and global preferences.

use crate::{repo, Error, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MsiUi {
    /// Progress bar only (`/passive`).
    #[default]
    Passive,
    /// Full installer wizard.
    Full,
}

/// Layout of the Applications page.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppsView {
    /// One full-width row per application.
    #[default]
    List,
    /// Grid of compact cards.
    Cards,
}

/// Optional per-OS string values (asset patterns, install script URLs).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PerOs {
    #[serde(default)]
    pub linux: Option<String>,
    #[serde(default)]
    pub windows: Option<String>,
}

/// Per-OS regular expressions overriding automatic asset selection.
pub type AssetPatterns = PerOs;

/// Per-OS HTTPS URLs of an install script (e.g. `https://zed.dev/install.sh`)
/// run instead of installing a release asset.
pub type InstallScripts = PerOs;

impl PerOs {
    pub fn for_os(&self, os: crate::assets::Os) -> Option<&str> {
        match os {
            crate::assets::Os::Linux => self.linux.as_deref(),
            crate::assets::Os::Windows => self.windows.as_deref(),
        }
        .map(str::trim)
        .filter(|p| !p.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackedApp {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    /// `owner/name`.
    pub repo: String,
    /// Display name; defaults to the repository name.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub include_prerelease: bool,
    #[serde(default)]
    pub asset_pattern: AssetPatterns,
    /// Debian package name, used to read the installed version via dpkg.
    #[serde(default)]
    pub package_name: Option<String>,
    /// Substring of the Windows "Programs and Features" display name.
    #[serde(default)]
    pub windows_display_name: Option<String>,
    /// Last version installed by this tool or detected on this machine.
    #[serde(default)]
    pub installed_version: Option<String>,
    /// A version the user chose to skip; newer versions are offered again.
    #[serde(default)]
    pub skipped_version: Option<String>,
    /// Include this app in automatic installs when [`Settings::auto_update`] is on.
    #[serde(default = "yes")]
    pub auto_update: bool,
    /// When set for the current OS, a new release is installed by downloading
    /// and running this script (like `curl -f <url> | sh`) instead of a
    /// release asset.
    #[serde(default)]
    pub install_script: InstallScripts,
}

fn yes() -> bool {
    true
}

impl TrackedApp {
    pub fn new(repo: &str) -> Result<Self> {
        Ok(Self {
            id: Uuid::new_v4(),
            repo: repo::parse_repo(repo)?,
            name: None,
            enabled: true,
            include_prerelease: false,
            asset_pattern: AssetPatterns::default(),
            package_name: None,
            windows_display_name: None,
            installed_version: None,
            skipped_version: None,
            auto_update: true,
            install_script: InstallScripts::default(),
        })
    }

    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| self.repo.split('/').nth(1).unwrap_or(&self.repo))
    }

    pub fn compiled_pattern(&self, os: crate::assets::Os) -> Result<Option<Regex>> {
        self.asset_pattern
            .for_os(os)
            .map(|p| {
                Regex::new(p).map_err(|source| Error::InvalidPattern {
                    pattern: p.to_string(),
                    source,
                })
            })
            .transpose()
    }

    /// Install script URL for `os`, if this app is installed by script there.
    pub fn install_script(&self, os: crate::assets::Os) -> Option<&str> {
        self.install_script.for_os(os)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    #[serde(default = "schema")]
    pub schema_version: u32,
    #[serde(default = "default_interval")]
    pub check_interval_hours: u32,
    #[serde(default = "yes")]
    pub autostart: bool,
    /// Show the window on the Updates page when an update is found.
    #[serde(default = "yes")]
    pub open_window_on_update: bool,
    /// Install updates as soon as a check finds them, without waiting for a
    /// click. The OS elevation prompt (polkit / UAC) still appears.
    #[serde(default)]
    pub auto_update: bool,
    #[serde(default)]
    pub msi_ui: MsiUi,
    /// Refuse to install assets for which the release publishes no checksum.
    #[serde(default)]
    pub require_checksum: bool,
    #[serde(default)]
    pub apps_view: AppsView,
    #[serde(default)]
    pub apps: Vec<TrackedApp>,
}

fn schema() -> u32 {
    SCHEMA_VERSION
}
fn default_interval() -> u32 {
    6
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            check_interval_hours: default_interval(),
            autostart: true,
            open_window_on_update: true,
            auto_update: false,
            msi_ui: MsiUi::default(),
            require_checksum: false,
            apps_view: AppsView::default(),
            apps: Vec::new(),
        }
    }
}

impl Settings {
    /// Normalises repositories, checks patterns, rejects duplicates and clamps
    /// the interval. Called before every save and on backup import.
    pub fn validate(mut self) -> Result<Self> {
        if self.schema_version > SCHEMA_VERSION {
            return Err(Error::InvalidBackup(format!(
                "settings schema {} is newer than supported ({SCHEMA_VERSION}); update this application",
                self.schema_version
            )));
        }
        self.schema_version = SCHEMA_VERSION;
        self.check_interval_hours = self.check_interval_hours.clamp(1, 24 * 7);
        let mut seen = std::collections::HashSet::new();
        for app in &mut self.apps {
            app.repo = repo::parse_repo(&app.repo)?;
            for os in [crate::assets::Os::Linux, crate::assets::Os::Windows] {
                app.compiled_pattern(os)?;
            }
            for field in [
                &mut app.install_script.linux,
                &mut app.install_script.windows,
            ] {
                *field = field
                    .as_deref()
                    .map(crate::script::normalize)
                    .transpose()?
                    .flatten();
            }
            if !seen.insert(app.repo.to_ascii_lowercase()) {
                return Err(Error::InvalidRepo(format!("{} is listed twice", app.repo)));
            }
            for field in [
                &mut app.package_name,
                &mut app.windows_display_name,
                &mut app.name,
            ] {
                if field.as_deref().is_some_and(|s| s.trim().is_empty()) {
                    *field = None;
                }
            }
            if let Some(pkg) = &app.package_name {
                if !pkg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
                {
                    return Err(Error::InvalidRepo(format!(
                        "invalid Debian package name '{pkg}'"
                    )));
                }
            }
        }
        Ok(self)
    }

    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(serde_json::from_str::<Settings>(&text)?.validate()?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Atomic write: temp file in the same directory, then rename.
    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, serde_json::to_string_pretty(self)?.as_bytes())
    }

    pub fn app(&self, id: Uuid) -> Option<&TrackedApp> {
        self.apps.iter().find(|a| a.id == id)
    }

    pub fn app_mut(&mut self, id: Uuid) -> Option<&mut TrackedApp> {
        self.apps.iter_mut().find(|a| a.id == id)
    }
}

pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut tmp, data)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/config.json");
        assert_eq!(Settings::load(&p).unwrap(), Settings::default());
        let mut s = Settings::default();
        s.apps.push(
            TrackedApp::new("https://github.com/storytold/wordcraft/releases/tag/v0.8.0").unwrap(),
        );
        s.save(&p).unwrap();
        let loaded = Settings::load(&p).unwrap();
        assert_eq!(loaded, s);
        assert_eq!(loaded.apps[0].repo, "storytold/wordcraft");
        assert_eq!(loaded.apps[0].display_name(), "wordcraft");
    }

    #[test]
    fn minimal_json_gets_defaults() {
        let s: Settings = serde_json::from_str(r#"{"apps":[{"repo":"a/b"}]}"#).unwrap();
        let s = s.validate().unwrap();
        assert!(s.apps[0].enabled);
        assert!(s.apps[0].auto_update);
        assert!(!s.auto_update);
        assert_eq!(s.check_interval_hours, 6);
    }

    #[test]
    fn rejects_bad_input() {
        let mut s = Settings::default();
        s.apps.push(TrackedApp::new("a/b").unwrap());
        s.apps.push(TrackedApp::new("A/B").unwrap());
        assert!(s.clone().validate().is_err(), "duplicate");
        s.apps.pop();
        s.apps[0].asset_pattern.linux = Some("(".into());
        assert!(s.clone().validate().is_err(), "regex");
        s.apps[0].asset_pattern.linux = None;
        s.apps[0].package_name = Some("bad name;rm".into());
        assert!(s.clone().validate().is_err(), "package");
        s.apps[0].package_name = None;
        s.apps[0].install_script.linux = Some("http://zed.dev/install.sh".into());
        assert!(s.clone().validate().is_err(), "script over http");
        s.apps[0].install_script.linux = Some(" https://zed.dev/install.sh ".into());
        let v = s.clone().validate().unwrap();
        assert_eq!(
            v.apps[0].install_script.linux.as_deref(),
            Some("https://zed.dev/install.sh")
        );
        s.apps[0].install_script.linux = Some("curl -f https://zed.dev/install.sh | sh".into());
        let v = s.clone().validate().unwrap();
        assert_eq!(
            v.apps[0].install_script.linux.as_deref(),
            Some("https://zed.dev/install.sh")
        );
        s.schema_version = 99;
        assert!(s.validate().is_err(), "schema");
    }
}
