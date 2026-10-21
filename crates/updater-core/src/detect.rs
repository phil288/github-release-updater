//! Reads installed versions from the OS package database.

use crate::config::TrackedApp;

/// Installed version of `app` on this machine, normalised for comparison.
pub async fn installed_version(app: &TrackedApp) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let pkg = app.package_name.as_deref()?;
        linux::dpkg_version(pkg)
            .await
            .map(|v| crate::version::normalize_installed(&v))
    }
    #[cfg(windows)]
    {
        let name = windows_entry_name(app);
        tokio::task::spawn_blocking(move || windows::registry_version(&name))
            .await
            .ok()
            .flatten()
            .map(|v| crate::version::normalize_installed(&v))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = app;
        None
    }
}

/// Substring of the "Programs and Features" display name identifying `app`.
#[cfg(windows)]
pub(crate) fn windows_entry_name(app: &TrackedApp) -> String {
    app.windows_display_name
        .clone()
        .unwrap_or_else(|| app.display_name().to_string())
}

/// Product code (`{GUID}`) of the MSI-installed entry matching `display_name`.
#[cfg(windows)]
pub(crate) fn msi_product_code(display_name: &str) -> Option<String> {
    windows::msi_product_code(display_name)
}

/// Package name declared inside a .deb file (`dpkg-deb -f file Package`).
pub async fn deb_package_name(path: &std::path::Path) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let out = tokio::process::Command::new("dpkg-deb")
            .arg("-f")
            .arg(path)
            .arg("Package")
            .output()
            .await
            .ok()?;
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && !name.is_empty()).then_some(name)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        None
    }
}

#[cfg(target_os = "linux")]
mod linux {
    pub async fn dpkg_version(pkg: &str) -> Option<String> {
        let out = tokio::process::Command::new("dpkg-query")
            .args(["-W", "-f=${db:Status-Abbrev}|${Version}", "--"])
            .arg(pkg)
            .output()
            .await
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        // "ii |1.2.3-1": only report fully installed packages.
        let (status, version) = text.trim().split_once('|')?;
        (status.starts_with("ii") && !version.is_empty()).then(|| version.to_string())
    }
}

#[cfg(windows)]
mod windows {
    use winreg::enums::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    };
    use winreg::RegKey;

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

    pub fn registry_version(display_name: &str) -> Option<String> {
        find_entry(display_name, |_, entry| {
            entry
                .get_value::<String, _>("DisplayVersion")
                .ok()
                .filter(|v| !v.is_empty())
        })
    }

    /// MSI installs are keyed by their product code and flagged `WindowsInstaller = 1`.
    pub fn msi_product_code(display_name: &str) -> Option<String> {
        find_entry(display_name, |sub, entry| {
            let msi: u32 = entry.get_value("WindowsInstaller").unwrap_or(0);
            (msi == 1 && crate::installer::is_product_code(sub)).then(|| sub.to_string())
        })
    }

    /// First uninstall entry whose DisplayName contains `display_name`
    /// (case-insensitive) and for which `pick` returns a value.
    fn find_entry<T>(display_name: &str, pick: impl Fn(&str, &RegKey) -> Option<T>) -> Option<T> {
        let needle = display_name.to_lowercase();
        let roots = [
            (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
            (HKEY_CURRENT_USER, 0),
        ];
        for (hive, flag) in roots {
            let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(UNINSTALL, KEY_READ | flag)
            else {
                continue;
            };
            for sub in key.enum_keys().flatten() {
                let Ok(entry) = key.open_subkey_with_flags(&sub, KEY_READ | flag) else {
                    continue;
                };
                let name: String = entry.get_value("DisplayName").unwrap_or_default();
                if name.to_lowercase().contains(&needle) {
                    if let Some(v) = pick(&sub, &entry) {
                        return Some(v);
                    }
                }
            }
        }
        None
    }
}
