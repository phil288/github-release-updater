use super::{is_product_code, Options, Outcome};
use crate::config::MsiUi;
use crate::{Error, Result};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const ERROR_SUCCESS_REBOOT_REQUIRED: u32 = 3010;
const ERROR_INSTALL_USEREXIT: u32 = 1602;

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Starts msiexec elevated (UAC prompt) and waits for it. Returns the exit code
/// or `None` when the user declined the UAC prompt.
fn run_elevated(params: &str) -> std::result::Result<Option<u32>, String> {
    let verb = wide("runas".as_ref());
    let file = wide("msiexec.exe".as_ref());
    let params = wide(params.as_ref());
    // SAFETY: all pointers reference live, NUL-terminated buffers for the
    // duration of the call; the struct is zero-initialised as the API expects.
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        if ShellExecuteExW(&mut info) == 0 {
            let err = GetLastError();
            return if err == ERROR_CANCELLED {
                Ok(None)
            } else {
                Err(format!("ShellExecuteExW failed: error {err}"))
            };
        }
        if info.hProcess.is_null() {
            return Err("msiexec did not start".into());
        }
        let wait = WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 0u32;
        let ok = GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        if wait != WAIT_OBJECT_0 || ok == 0 {
            return Err("failed to wait for msiexec".into());
        }
        Ok(Some(code))
    }
}

pub async fn install_msi(
    file: &Path,
    opts: Options<'_>,
    on_line: impl FnMut(String) + Send + 'static,
) -> Result<Outcome> {
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("install");
    // File names were sanitised on download, so quoting is sufficient.
    let action = format!("/i \"{}\"", file.display());
    run_msiexec(
        &action,
        stem,
        opts,
        Outcome::Installed,
        Outcome::InstalledRebootRequired,
        on_line,
    )
    .await
}

/// `msiexec /x {PRODUCT-CODE}`. `product_code` must be a braced GUID.
pub async fn uninstall_msi(
    product_code: &str,
    opts: Options<'_>,
    on_line: impl FnMut(String) + Send + 'static,
) -> Result<Outcome> {
    if !is_product_code(product_code) {
        return Err(Error::Installer(format!(
            "invalid MSI product code '{product_code}'"
        )));
    }
    let action = format!("/x {product_code}");
    run_msiexec(
        &action,
        "uninstall",
        opts,
        Outcome::Uninstalled,
        Outcome::UninstalledRebootRequired,
        on_line,
    )
    .await
}

async fn run_msiexec(
    action: &str,
    log_stem: &str,
    opts: Options<'_>,
    success: Outcome,
    success_reboot: Outcome,
    mut on_line: impl FnMut(String) + Send + 'static,
) -> Result<Outcome> {
    std::fs::create_dir_all(opts.log_dir)?;
    let log: PathBuf = opts.log_dir.join(format!(
        "{log_stem}-{}.log",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    let ui = match opts.msi_ui {
        MsiUi::Passive => "/passive",
        MsiUi::Full => "",
    };
    let params = format!("{action} {ui} /norestart /l*v \"{}\"", log.display());
    on_line(format!("> msiexec {params}"));
    on_line("Waiting for administrator approval (UAC)…".into());

    let code = tokio::task::spawn_blocking(move || run_elevated(&params))
        .await
        .map_err(|e| Error::Installer(e.to_string()))?
        .map_err(Error::Installer)?;
    on_line(format!("Installer log: {}", log.display()));
    Ok(match code {
        None => Outcome::Cancelled,
        Some(0) => success,
        Some(ERROR_SUCCESS_REBOOT_REQUIRED) => success_reboot,
        Some(ERROR_INSTALL_USEREXIT) => Outcome::Cancelled,
        Some(c) => Outcome::Failed {
            code: Some(c as i32),
            message: format!("msiexec exited with code {c}; see {}", log.display()),
        },
    })
}
