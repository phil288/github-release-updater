//! Runs a downloaded installer (or an uninstall) with elevation, in the foreground.
//!
//! * Linux: `pkexec apt-get install <file.deb>` / `apt-get remove <package>` —
//!   polkit shows its password dialog.
//! * Windows: `msiexec /i <file.msi>` / `msiexec /x {product-code}` started with
//!   the `runas` verb — UAC prompt.

use crate::config::{MsiUi, TrackedApp};
use crate::Result;
use serde::Serialize;
use std::path::Path;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Installed,
    InstalledRebootRequired,
    Uninstalled,
    UninstalledRebootRequired,
    /// The user dismissed the password / UAC prompt or the installer UI.
    Cancelled,
    Failed {
        code: Option<i32>,
        message: String,
    },
}

pub struct Options<'a> {
    pub msi_ui: MsiUi,
    /// Directory for installer logs (Windows MSI log).
    pub log_dir: &'a Path,
}

/// Installs `file`. `on_line` receives installer output lines as they appear.
pub async fn install(
    file: &Path,
    opts: Options<'_>,
    on_line: impl FnMut(String) + Send + 'static,
) -> Result<Outcome> {
    #[cfg(target_os = "linux")]
    {
        let _ = &opts;
        linux::install_deb(file, on_line).await
    }
    #[cfg(windows)]
    {
        windows::install_msi(file, opts, on_line).await
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (file, opts, on_line);
        Err(crate::Error::Installer("unsupported platform".into()))
    }
}

/// Uninstalls `app` through the OS package manager. Linux needs the app's
/// Debian package name; Windows looks up the MSI product code of the
/// "Programs and Features" entry matching the app.
pub async fn uninstall(
    app: &TrackedApp,
    opts: Options<'_>,
    on_line: impl FnMut(String) + Send + 'static,
) -> Result<Outcome> {
    #[cfg(target_os = "linux")]
    {
        let _ = &opts;
        let pkg = app.package_name.as_deref().ok_or_else(|| {
            crate::Error::Installer(
                "the Debian package name is unknown; set it in the app's settings".into(),
            )
        })?;
        linux::uninstall_deb(pkg, on_line).await
    }
    #[cfg(windows)]
    {
        let name = crate::detect::windows_entry_name(app);
        let code = tokio::task::spawn_blocking({
            let name = name.clone();
            move || crate::detect::msi_product_code(&name)
        })
        .await
        .map_err(|e| crate::Error::Installer(e.to_string()))?
        .ok_or_else(|| {
            crate::Error::Installer(format!(
                "no MSI installation matching '{name}' was found; \
                 uninstall it from Settings > Apps instead"
            ))
        })?;
        windows::uninstall_msi(&code, opts, on_line).await
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (app, opts, on_line);
        Err(crate::Error::Installer("unsupported platform".into()))
    }
}

/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
pub fn is_product_code(s: &str) -> bool {
    let Some(inner) = s.strip_prefix('{').and_then(|s| s.strip_suffix('}')) else {
        return false;
    };
    let groups: Vec<&str> = inner.split('-').collect();
    groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12])
        && groups
            .iter()
            .all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_code_shape() {
        assert!(is_product_code("{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9}"));
        assert!(!is_product_code("0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9"));
        assert!(!is_product_code("{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F}"));
        assert!(!is_product_code("{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8FG}"));
        assert!(!is_product_code("{x\" /qn REMOVE=ALL}"));
        assert!(!is_product_code("MyApp"));
    }
}
