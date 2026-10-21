<#
.SYNOPSIS
  Uninstalls GitHub Release Updater from Windows.

.DESCRIPTION
  Removes the MSI installation (UAC prompt) and your startup entry. Settings are kept
  unless -Purge is given. Run from a normal (non-admin) PowerShell:
    powershell -ExecutionPolicy Bypass -File scripts\uninstall.ps1 [-Purge] [-Yes]

.PARAMETER Purge
  Also delete settings, token, downloads and logs.

.PARAMETER Yes
  Do not ask for confirmation.
#>
param(
  [switch] $Purge,
  [switch] $Yes
)

$ErrorActionPreference = "Stop"
$AppName = "GitHub Release Updater"
$AppId = "dev.phil.gh-release-updater"
$ProcessName = "gh-release-updater"

function Info($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Fail($msg) { Write-Host "error: $msg" -ForegroundColor Red; exit 1 }

# MSI product codes of installed copies (per-machine and per-user, 64/32-bit views).
function Get-Installations {
  $roots = @(
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall"
  )
  foreach ($root in $roots) {
    if (-not (Test-Path $root)) { continue }
    Get-ChildItem $root | ForEach-Object {
      $p = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
      if ($p.DisplayName -eq $AppName -and $_.PSChildName -match '^\{[0-9A-Fa-f-]{36}\}$') {
        [pscustomobject]@{ ProductCode = $_.PSChildName; Version = $p.DisplayVersion }
      }
    }
  }
}

$installs = @(Get-Installations | Sort-Object ProductCode -Unique)
$userPaths = @((Join-Path $env:APPDATA $AppId), (Join-Path $env:LOCALAPPDATA $AppId))
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$approvedKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"

Write-Host "This will:"
if ($installs.Count) { $installs | ForEach-Object { Write-Host "  - uninstall $AppName $($_.Version) $($_.ProductCode) (UAC)" } }
else { Write-Host "  - (no MSI installation found)" }
Write-Host "  - stop the running app and remove your startup entry"
if ($Purge) { $userPaths | ForEach-Object { Write-Host "  - delete $_" } }
if (-not $Yes) {
  $answer = Read-Host "Continue? [y/N]"
  if ($answer -notmatch '^[Yy]$') { Write-Host "Aborted."; exit 1 }
}

$running = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
if ($running) {
  Info "Stopping running instance"
  $running | Stop-Process -Force
}

foreach ($i in $installs) {
  $log = Join-Path $env:TEMP ("gh-release-updater-uninstall-{0:yyyyMMdd-HHmmss}.log" -f (Get-Date))
  Info "Uninstalling $($i.ProductCode) (approve the UAC prompt)"
  try {
    $p = Start-Process msiexec.exe -Verb RunAs -Wait -PassThru `
      -ArgumentList @("/x", $i.ProductCode, "/passive", "/norestart", "/l*v", "`"$log`"")
  } catch {
    Fail "uninstall cancelled (UAC declined)"
  }
  switch ($p.ExitCode) {
    0 { }
    3010 { Info "Restart Windows to complete the removal." }
    1605 { Info "Already removed." }
    1602 { Fail "uninstall cancelled" }
    default { Fail "msiexec exited with code $($p.ExitCode); log: $log" }
  }
}

# The startup entry is per user and not owned by the MSI.
foreach ($key in @($runKey, $approvedKey)) {
  if (Get-ItemProperty -Path $key -Name $AppName -ErrorAction SilentlyContinue) {
    Info "Removing startup entry ($key)"
    Remove-ItemProperty -Path $key -Name $AppName
  }
}

if ($Purge) {
  foreach ($path in $userPaths) {
    if (Test-Path -LiteralPath $path) {
      Info "Deleting $path"
      Remove-Item -LiteralPath $path -Recurse -Force
    }
  }
} else {
  Info "Settings kept in $($userPaths[0]) (use -Purge to delete)"
}
Info "Done."
