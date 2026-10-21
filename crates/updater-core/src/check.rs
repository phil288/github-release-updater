//! Update check and install pipeline shared by the scheduler and the UI.

use crate::assets::{self, Asset, Platform};
use crate::checksum::{self, Source};
use crate::config::{Settings, TrackedApp};
use crate::github::{self, Client, Release};
use crate::installer::{self, Outcome};
use crate::{download, version, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// An update that can be installed on this machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub app_id: Uuid,
    pub app_name: String,
    pub repo: String,
    pub installed_version: Option<String>,
    pub tag: String,
    pub release_name: Option<String>,
    pub html_url: String,
    pub published_at: Option<String>,
    pub notes: Option<String>,
    pub prerelease: bool,
    pub asset: Asset,
    /// Every asset of the release; needed to locate checksum files.
    pub release_assets: Vec<Asset>,
    /// Releases between the installed version and `tag` that are jumped over
    /// (newest first). Updates always go straight to the highest version.
    #[serde(default)]
    pub intermediate_tags: Vec<String>,
}

/// Result of looking at one tracked app.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CheckResult {
    UpToDate { tag: String },
    Skipped { tag: String },
    Available(Box<UpdateInfo>),
    NoRelease,
    NoMatchingAsset { tag: String, assets: Vec<String> },
    Error { message: String },
}

/// Preview shown while editing an app: which asset would be picked.
#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub tag: String,
    pub selected: Option<String>,
    pub installer_assets: Vec<String>,
    pub checksum: Option<String>,
}

pub async fn preview(
    client: &Client,
    app: &TrackedApp,
    platform: Platform,
) -> Result<Option<Preview>> {
    let pattern = app.compiled_pattern(platform.os)?;
    let releases = client.releases(&app.repo).await?;
    let Some(release) = github::pick_highest(&releases, app.include_prerelease) else {
        return Ok(None);
    };
    let selected = assets::select(&release.assets, platform, pattern.as_ref());
    let checksum = selected
        .and_then(|a| checksum::find_source(a, &release.assets))
        .map(|s| match s {
            Source::Digest(_) => "GitHub asset digest".to_string(),
            Source::File(f) => f.name.clone(),
        });
    let ext = platform.extension();
    Ok(Some(Preview {
        tag: release.tag_name.clone(),
        selected: selected.map(|a| a.name.clone()),
        installer_assets: release
            .assets
            .iter()
            .filter(|a| a.name.to_ascii_lowercase().ends_with(ext))
            .map(|a| a.name.clone())
            .collect(),
        checksum,
    }))
}

pub async fn check_app(client: &Client, app: &TrackedApp, platform: Platform) -> CheckResult {
    match check_inner(client, app, platform).await {
        Ok(r) => r,
        Err(e) => CheckResult::Error {
            message: e.to_string(),
        },
    }
}

async fn check_inner(client: &Client, app: &TrackedApp, platform: Platform) -> Result<CheckResult> {
    let pattern = app.compiled_pattern(platform.os)?;
    let releases = client.releases(&app.repo).await?;
    Ok(evaluate(app, &releases, platform, pattern.as_ref()))
}

/// Decides what to offer: only the highest eligible release, never one of the
/// versions in between.
fn evaluate(
    app: &TrackedApp,
    releases: &[Release],
    platform: Platform,
    pattern: Option<&regex::Regex>,
) -> CheckResult {
    let Some(release) = github::pick_highest(releases, app.include_prerelease) else {
        return CheckResult::NoRelease;
    };
    let release = release.clone();
    let tag = release.tag_name.clone();
    let installed = app.installed_version.as_deref();
    if !version::is_newer(&tag, installed) {
        return CheckResult::UpToDate { tag };
    }
    // Skipping a version also skips everything below it.
    if app
        .skipped_version
        .as_deref()
        .is_some_and(|s| !version::compare(&tag, s).is_gt())
    {
        return CheckResult::Skipped { tag };
    }
    let mut intermediate: Vec<&Release> = releases
        .iter()
        .filter(|r| github::eligible(r, app.include_prerelease))
        .filter(|r| version::is_newer(&r.tag_name, installed))
        .filter(|r| version::compare(&r.tag_name, &tag).is_lt())
        .collect();
    intermediate.sort_by(|a, b| version::compare(&b.tag_name, &a.tag_name));
    let intermediate_tags = intermediate
        .into_iter()
        .map(|r| r.tag_name.clone())
        .collect();
    let Some(asset) = assets::select(&release.assets, platform, pattern).cloned() else {
        return CheckResult::NoMatchingAsset {
            tag,
            assets: release.assets.iter().map(|a| a.name.clone()).collect(),
        };
    };
    CheckResult::Available(Box::new(UpdateInfo {
        app_id: app.id,
        app_name: app.display_name().to_string(),
        repo: app.repo.clone(),
        installed_version: app.installed_version.clone(),
        tag,
        release_name: release.name,
        html_url: release.html_url,
        published_at: release.published_at,
        notes: release.body.map(|b| b.chars().take(4000).collect()),
        prerelease: release.prerelease,
        asset,
        release_assets: release.assets,
        intermediate_tags,
    }))
}

/// Progress reported while installing.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Progress {
    Downloading { downloaded: u64, total: Option<u64> },
    Verifying,
    Installing,
    Log { line: String },
}

/// Result of a successful pipeline run (the installer may still have been cancelled).
#[derive(Debug, Clone, Serialize)]
pub struct InstallReport {
    pub outcome: Outcome,
    /// Release that was installed (may be newer than the one the user clicked).
    pub tag: String,
    pub file: PathBuf,
    pub verified: bool,
    /// Debian package name read from the .deb, to fill in `package_name`.
    pub package_name: Option<String>,
}

/// Download → verify → install.
pub async fn download_and_install(
    client: &Client,
    update: &UpdateInfo,
    settings: &Settings,
    cache_dir: &Path,
    log_dir: &Path,
    progress: impl Fn(Progress) + Send + Sync + Clone + 'static,
) -> Result<InstallReport> {
    let repo_dir = update.repo.replace('/', "_");
    let dest = cache_dir
        .join(download::sanitize_file_name(&repo_dir))
        .join(download::sanitize_file_name(&update.tag))
        .join(download::sanitize_file_name(&update.asset.name));

    // Expected checksum is resolved before the download so a missing one fails fast.
    let expected = match checksum::find_source(&update.asset, &update.release_assets) {
        Some(Source::Digest(h)) => Some(h),
        Some(Source::File(f)) => {
            let text = client.fetch_text(&f.browser_download_url).await?;
            checksum::expected_from_file(&text, &update.asset.name)
        }
        None => None,
    };
    if expected.is_none() && settings.require_checksum {
        return Err(Error::ChecksumMissing(update.asset.name.clone()));
    }

    let http = download::download_client()?;
    {
        let p = progress.clone();
        download::download(
            &http,
            &update.asset.browser_download_url,
            &dest,
            move |d, t| {
                p(Progress::Downloading {
                    downloaded: d,
                    total: t,
                })
            },
        )
        .await?;
    }

    progress(Progress::Verifying);
    let actual = checksum::sha256_file(&dest).await?;
    let verified = match &expected {
        Some(exp) if *exp != actual => {
            let _ = tokio::fs::remove_file(&dest).await;
            return Err(Error::ChecksumMismatch {
                file: update.asset.name.clone(),
                expected: exp.clone(),
                actual,
            });
        }
        Some(_) => {
            progress(Progress::Log {
                line: format!("SHA-256 verified: {actual}"),
            });
            true
        }
        None => {
            progress(Progress::Log {
                line: format!("No checksum published; SHA-256 is {actual}"),
            });
            false
        }
    };
    let package_name = crate::detect::deb_package_name(&dest).await;

    progress(Progress::Installing);
    let p = progress.clone();
    let outcome = installer::install(
        &dest,
        installer::Options {
            msi_ui: settings.msi_ui,
            log_dir,
        },
        move |line| p(Progress::Log { line }),
    )
    .await?;
    Ok(InstallReport {
        outcome,
        tag: update.tag.clone(),
        file: dest,
        verified,
        package_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Arch, Os};

    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.into(),
            name: None,
            prerelease: false,
            draft: false,
            html_url: "u".into(),
            published_at: None,
            body: None,
            assets: crate::assets::tests::wordcraft_assets(),
        }
    }

    #[test]
    fn evaluates() {
        let p = Platform {
            os: Os::Linux,
            arch: Arch::X64,
        };
        let mut app = TrackedApp::new("storytold/wordcraft").unwrap();
        app.installed_version = Some("0.7.0".into());
        match evaluate(&app, &[release("v0.8.0")], p, None) {
            CheckResult::Available(u) => {
                assert_eq!(u.asset.name, "wordcraft-0.8.0-linux-x86_64.deb")
            }
            other => panic!("{other:?}"),
        }
        app.skipped_version = Some("v0.8.0".into());
        assert!(matches!(
            evaluate(&app, &[release("v0.8.0")], p, None),
            CheckResult::Skipped { .. }
        ));
        app.installed_version = Some("0.8.0".into());
        assert!(matches!(
            evaluate(&app, &[release("v0.8.0")], p, None),
            CheckResult::UpToDate { .. }
        ));
        app.installed_version = None;
        app.skipped_version = None;
        let riscv = Platform {
            os: Os::Linux,
            arch: Arch::X86,
        };
        assert!(matches!(
            evaluate(&app, &[release("v0.8.0")], riscv, None),
            CheckResult::NoMatchingAsset { .. }
        ));
    }

    fn available(r: CheckResult) -> Box<UpdateInfo> {
        match r {
            CheckResult::Available(u) => u,
            other => panic!("expected an update, got {other:?}"),
        }
    }

    #[test]
    fn jumps_straight_to_highest() {
        let p = Platform {
            os: Os::Linux,
            arch: Arch::X64,
        };
        let mut app = TrackedApp::new("storytold/wordcraft").unwrap();
        app.installed_version = Some("0.5.0".into());
        let mut beta = release("v1.0.0-beta.1");
        beta.prerelease = true;
        let mut draft = release("v2.0.0");
        draft.draft = true;
        // Newest-published first, as GitHub lists them: a backport after the highest.
        let rs = [
            release("v0.7.5"),
            draft,
            beta,
            release("v0.9.0"),
            release("v0.8.0"),
            release("v0.6.0"),
            release("v0.5.0"),
            release("v0.4.0"),
        ];

        let u = available(evaluate(&app, &rs, p, None));
        assert_eq!(u.tag, "v0.9.0");
        assert_eq!(u.intermediate_tags, ["v0.8.0", "v0.7.5", "v0.6.0"]);

        app.include_prerelease = true;
        let u = available(evaluate(&app, &rs, p, None));
        assert_eq!(u.tag, "v1.0.0-beta.1");
        assert_eq!(
            u.intermediate_tags.first().map(String::as_str),
            Some("v0.9.0")
        );
        app.include_prerelease = false;

        // Skipping v0.9.0 suppresses it and everything below, but not a later v0.10.0.
        app.skipped_version = Some("v0.9.0".into());
        assert!(matches!(
            evaluate(&app, &rs, p, None),
            CheckResult::Skipped { .. }
        ));
        let mut newer = rs.to_vec();
        newer.insert(0, release("v0.10.0"));
        assert_eq!(available(evaluate(&app, &newer, p, None)).tag, "v0.10.0");

        assert!(matches!(
            evaluate(&app, &[], p, None),
            CheckResult::NoRelease
        ));
    }
}
