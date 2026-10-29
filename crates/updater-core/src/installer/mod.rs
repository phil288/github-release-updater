//! Runs a downloaded installer (or an uninstall) with elevation, in the foreground.
//!
//! * Linux: `pkexec apt-get install <file.deb>` / `apt-get remove <package>` —
//!   polkit shows its password dialog.
//! * Windows: `msiexec /i <file.msi>` / `msiexec /x {product-code}` started with
//!   the `runas` verb — UAC prompt.
//! * Install scripts ([`run_script`]): `sh install.sh` / `powershell -File
//!   install.ps1`, run as the current user without elevation.

use crate::config::{MsiUi, TrackedApp};
use crate::{Error, Result};
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

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

/// Runs a downloaded install script as the current user, non-interactively
/// (stdin is closed), streaming its output to `on_line`. Exit code 0 means
/// installed.
pub async fn run_script(
    file: &Path,
    mut on_line: impl FnMut(String) + Send + 'static,
) -> Result<Outcome> {
    #[cfg(windows)]
    let (mut cmd, shown) = {
        let mut cmd = Command::new("powershell.exe");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(file);
        // CREATE_NO_WINDOW: no console window pops up from the tray app.
        cmd.creation_flags(0x0800_0000);
        (cmd, "powershell")
    };
    #[cfg(not(windows))]
    let (mut cmd, shown) = {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg(file);
        (cmd, "sh")
    };
    if let Some(dir) = file.parent() {
        cmd.current_dir(dir);
    }
    on_line(format!("$ {shown} {}", file.display()));
    let (code, tail) = run_streaming(cmd, shown, on_line).await?;
    Ok(match code {
        Some(0) => Outcome::Installed,
        code => Outcome::Failed {
            code,
            message: tail
                .last()
                .cloned()
                .unwrap_or_else(|| format!("install script failed with exit code {code:?}")),
        },
    })
}

/// Spawns `cmd` with stdin closed, forwards stdout and stderr lines to
/// `on_line`, and returns the exit code with the last 20 lines.
pub(crate) async fn run_streaming(
    mut cmd: Command,
    name: &str,
    mut on_line: impl FnMut(String) + Send + 'static,
) -> Result<(Option<i32>, Vec<String>)> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| Error::Installer(format!("failed to start {name}: {e}")))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    for reader in [
        Box::new(stdout) as Box<dyn tokio::io::AsyncRead + Unpin + Send>,
        Box::new(stderr),
    ] {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(line);
            }
        });
    }
    drop(tx);
    let mut tail: Vec<String> = Vec::new();
    while let Some(line) = rx.recv().await {
        tail.push(line.clone());
        if tail.len() > 20 {
            tail.remove(0);
        }
        on_line(line);
    }
    let status = child.wait().await?;
    Ok((status.code(), tail))
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

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_script_and_maps_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("ok.sh");
        std::fs::write(&ok, "echo installing\necho done >&2\n").unwrap();
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let l = lines.clone();
        let out = run_script(&ok, move |line| l.lock().unwrap().push(line))
            .await
            .unwrap();
        assert_eq!(out, Outcome::Installed);
        let lines = lines.lock().unwrap().clone();
        assert!(lines.contains(&"installing".to_string()));
        assert!(lines.contains(&"done".to_string()));

        let bad = dir.path().join("bad.sh");
        std::fs::write(&bad, "echo 'curl: (22) 404'\nexit 3\n").unwrap();
        match run_script(&bad, |_| {}).await.unwrap() {
            Outcome::Failed { code, message } => {
                assert_eq!(code, Some(3));
                assert_eq!(message, "curl: (22) 404");
            }
            other => panic!("{other:?}"),
        }
    }
}
