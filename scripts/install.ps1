<#
.SYNOPSIS
  Installs GitHub Release Updater on Windows.

.DESCRIPTION
  Default: builds the .msi from source, then installs it (UAC prompt).
  -Release OWNER/REPO : downloads the latest released .msi for this CPU and installs it.
  -Package FILE.msi   : installs an existing .msi.

  Run from a normal (non-admin) PowerShell; elevation is requested only for msiexec:
    powershell -ExecutionPolicy Bypass -File scripts\install.ps1

.PARAMETER SkipDeps
  Do not install Rust, Node.js or the MSVC build tools with winget.
#>
[CmdletBinding(DefaultParameterSetName = "Source")]
param(
  [Parameter(ParameterSetName = "Release", Mandatory)] [ValidatePattern('^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$')] [string] $Release,
  [Parameter(ParameterSetName = "Package", Mandatory)] [string] $Package,
  [Parameter(ParameterSetName = "Source")] [switch] $SkipDeps
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"   # Invoke-WebRequest is much faster without it
$Root = Split-Path -Parent $PSScriptRoot

function Info($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Fail($msg) { Write-Host "error: $msg" -ForegroundColor Red; exit 1 }

function Refresh-Path {
  $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
              [Environment]::GetEnvironmentVariable("Path", "User") + ";" +
              "$env:USERPROFILE\.cargo\bin"
}

function Install-Msi([string] $msi) {
  if (-not (Test-Path -LiteralPath $msi)) { Fail "package not found: $msi" }
  $msi = (Resolve-Path -LiteralPath $msi).Path
  $log = Join-Path $env:TEMP ("gh-release-updater-install-{0:yyyyMMdd-HHmmss}.log" -f (Get-Date))
  Info "Installing $(Split-Path -Leaf $msi) (approve the UAC prompt)"
  try {
    $p = Start-Process msiexec.exe -Verb RunAs -Wait -PassThru `
      -ArgumentList @("/i", "`"$msi`"", "/passive", "/norestart", "/l*v", "`"$log`"")
  } catch {
    Fail "installation cancelled (UAC declined)"
  }
  switch ($p.ExitCode) {
    0 { Info "Installed. Start it from the Start menu (it will add itself to startup apps)." }
    3010 { Info "Installed. Restart Windows to complete the installation." }
    1602 { Fail "installation cancelled" }
    default { Fail "msiexec exited with code $($p.ExitCode); log: $log" }
  }
}

function Ensure-Winget([string] $id, [string] $name, [string[]] $extra = @()) {
  if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    Fail "winget not found; install $name manually or update 'App Installer' from the Microsoft Store"
  }
  Info "Installing $name with winget"
  winget install --id $id -e --accept-source-agreements --accept-package-agreements @extra
  # 0x8A15002B: no applicable upgrade, 0x8A150061: already installed.
  if ($LASTEXITCODE -notin @(0, -1978335189, -1978335135)) {
    Fail "winget failed to install $name (exit $LASTEXITCODE)"
  }
  Refresh-Path
}

function Test-MsvcTools {
  $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path $vswhere)) { return $false }
  $found = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  return [bool]$found
}

function Build-FromSource {
  Set-Location $Root
  Refresh-Path
  if (-not $SkipDeps) {
    if (-not (Test-MsvcTools)) {
      Ensure-Winget "Microsoft.VisualStudio.2022.BuildTools" "MSVC build tools" @(
        "--override", "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
      Ensure-Winget "Rustlang.Rustup" "Rust (rustup)"
      rustup default stable
    }
    if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
      Ensure-Winget "OpenJS.NodeJS.LTS" "Node.js LTS"
    }
  }
  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Fail "cargo not found; install Rust from https://rustup.rs" }
  if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Fail "Node.js not found; install Node.js 20+" }
  $nodeMajor = [int](node -p "process.versions.node.split('.')[0]")
  if ($nodeMajor -lt 20) { Fail "Node.js 20+ required, found $(node --version)" }

  Info "Installing npm packages"
  npm ci
  if ($LASTEXITCODE -ne 0) { Fail "npm ci failed" }
  Info "Building release package (this takes a few minutes)"
  npx tauri build --bundles msi
  if ($LASTEXITCODE -ne 0) { Fail "tauri build failed" }
  $msi = Get-ChildItem "$Root\target\release\bundle\msi\*.msi" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
  if (-not $msi) { Fail "build finished but no .msi found in target\release\bundle\msi" }
  Install-Msi $msi.FullName
}

function Install-FromRelease {
  $arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64|amd64|x64|win64" }
    "ARM64" { "aarch64|arm64" }
    "x86"   { "x86|i386|i686|win32" }
    default { Fail "unsupported architecture $env:PROCESSOR_ARCHITECTURE" }
  }
  Info "Looking up latest release of $Release"
  $rel = Invoke-RestMethod -Headers @{ Accept = "application/vnd.github+json" } `
    -Uri "https://api.github.com/repos/$Release/releases/latest"
  $msis = @($rel.assets | Where-Object { $_.name -match '\.msi$' })
  $pattern = "(?i)(^|[^a-z0-9])($arch)([^a-z0-9]|$)"
  $asset = $msis | Where-Object { $_.name -match $pattern } | Select-Object -First 1
  if (-not $asset -and $msis.Count -eq 1) { $asset = $msis[0] }
  if (-not $asset) { Fail "no .msi asset for $env:PROCESSOR_ARCHITECTURE in $($rel.tag_name)" }
  if (-not $asset.browser_download_url.StartsWith("https://github.com/")) {
    Fail "unexpected download URL: $($asset.browser_download_url)"
  }

  $dir = Join-Path $env:TEMP ("gh-release-updater-" + [guid]::NewGuid())
  New-Item -ItemType Directory $dir | Out-Null
  try {
    $file = Join-Path $dir ($asset.name -replace '[^A-Za-z0-9._-]', '_')
    Info "Downloading $($asset.name)"
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $file
    if ($asset.digest -and $asset.digest.StartsWith("sha256:")) {
      $expected = $asset.digest.Substring(7)
      $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash
      if ($actual -ne $expected) { Fail "SHA-256 mismatch for $($asset.name)" }  # -ne is case-insensitive
      Info "SHA-256 verified"
    } else {
      Info "No digest published for $($asset.name); skipping checksum verification"
    }
    Install-Msi $file
  } finally {
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
  }
}

switch ($PSCmdlet.ParameterSetName) {
  "Source"  { Build-FromSource }
  "Release" { Install-FromRelease }
  "Package" { Install-Msi $Package }
}
