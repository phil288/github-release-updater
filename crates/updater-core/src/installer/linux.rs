use super::Outcome;
use crate::{Error, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::process::Command;

/// apt's unprivileged `_apt` user must be able to read the file, and root
/// should not install from a path the user can still modify. Copy the package
/// into a fresh directory under /tmp, readable but not writable by others.
fn stage(file: &Path) -> Result<(tempfile::TempDir, std::path::PathBuf)> {
    let dir = tempfile::Builder::new()
        .prefix("gh-release-updater-")
        .tempdir_in("/tmp")?;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755))?;
    let name = file
        .file_name()
        .ok_or_else(|| Error::Installer("bad file name".into()))?;
    let staged = dir.path().join(name);
    std::fs::copy(file, &staged)?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o644))?;
    Ok((dir, staged))
}

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|d| Path::new(d).join(cmd))
        .find(|p| p.is_file())
}

pub async fn install_deb(
    file: &Path,
    on_line: impl FnMut(String) + Send + 'static,
) -> Result<Outcome> {
    let (_guard, staged) = stage(file)?;
    run_apt("install", staged.as_os_str(), Outcome::Installed, on_line).await
}

/// `pkexec apt-get remove -y <package>`: removes the package, keeps its
/// system-wide configuration files (like `apt remove`).
pub async fn uninstall_deb(
    package: &str,
    on_line: impl FnMut(String) + Send + 'static,
) -> Result<Outcome> {
    // Debian policy: package names start with an alphanumeric character, so a
    // valid name can never be read as an apt-get option.
    if !package.starts_with(|c: char| c.is_ascii_alphanumeric())
        || !package
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
    {
        return Err(Error::Installer(format!(
            "invalid Debian package name '{package}'"
        )));
    }
    run_apt("remove", package.as_ref(), Outcome::Uninstalled, on_line).await
}

/// Runs `pkexec env DEBIAN_FRONTEND=noninteractive apt-get <action> -y <target>`,
/// streaming its output to `on_line`. Exit code 0 maps to `success`.
async fn run_apt(
    action: &str,
    target: &std::ffi::OsStr,
    success: Outcome,
    mut on_line: impl FnMut(String) + Send + 'static,
) -> Result<Outcome> {
    let pkexec = which("pkexec").ok_or_else(|| {
        Error::Installer("pkexec not found: install the 'pkexec' (polkit) package".into())
    })?;
    on_line(format!(
        "$ pkexec apt-get {action} -y {}",
        target.to_string_lossy()
    ));

    let mut cmd = Command::new(pkexec);
    cmd.arg("/usr/bin/env")
        .arg("DEBIAN_FRONTEND=noninteractive")
        .arg("/usr/bin/apt-get")
        .args([action, "-y", "-o", "Dpkg::Use-Pty=0", "-o", "APT::Color=0"])
        .arg(target);
    let (code, tail) = super::run_streaming(cmd, "pkexec", on_line).await?;
    Ok(match code {
        Some(0) => success,
        // pkexec: 126 = authorization dismissed/denied, 127 = auth failed or no agent.
        Some(126) => Outcome::Cancelled,
        Some(127) => Outcome::Failed {
            code: Some(127),
            message: "Authentication failed or no polkit agent is running".into(),
        },
        code => Outcome::Failed {
            code,
            message: tail
                .iter()
                .rev()
                .find(|l| l.starts_with("E:") || l.contains("error"))
                .cloned()
                .unwrap_or_else(|| "apt-get failed".into()),
        },
    })
}
