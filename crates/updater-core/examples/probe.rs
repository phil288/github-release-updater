//! Manual end-to-end probe against the real GitHub API, without installing:
//!
//!     cargo run -p updater-core --example probe -- storytold/wordcraft [--download]
//!
//! Prints the latest release, the asset chosen for this machine and its checksum
//! source; with `--download` it also downloads and verifies the asset.

use updater_core::assets::Platform;
use updater_core::check::{self, CheckResult};
use updater_core::checksum::{self, Source};
use updater_core::config::TrackedApp;
use updater_core::{download, github};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = args
        .first()
        .ok_or("usage: probe <owner/name|url> [--download]")?;
    let client = github::Client::new(std::env::var("GITHUB_TOKEN").ok())?;
    let app = TrackedApp::new(repo)?;
    let platform = Platform::current();
    println!("platform: {platform:?}");
    match check::preview(&client, &app, platform).await? {
        None => println!("{}: no release", app.repo),
        Some(p) => println!(
            "{}: {} -> {:?} (checksum: {:?})",
            app.repo, p.tag, p.selected, p.checksum
        ),
    }
    let CheckResult::Available(u) = check::check_app(&client, &app, platform).await else {
        return Ok(());
    };
    if !args.iter().any(|a| a == "--download") {
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    let dest = dir.path().join(download::sanitize_file_name(&u.asset.name));
    download::download(
        &download::download_client()?,
        &u.asset.browser_download_url,
        &dest,
        |_, _| {},
    )
    .await?;
    let actual = checksum::sha256_file(&dest).await?;
    let expected = match checksum::find_source(&u.asset, &u.release_assets) {
        Some(Source::Digest(h)) => Some(h),
        Some(Source::File(f)) => checksum::expected_from_file(
            &client.fetch_text(&f.browser_download_url).await?,
            &u.asset.name,
        ),
        None => None,
    };
    println!(
        "downloaded {} ({} bytes)",
        u.asset.name,
        std::fs::metadata(&dest)?.len()
    );
    println!(
        "sha256 {actual} expected {expected:?} -> {}",
        if expected.as_deref() == Some(&actual) {
            "OK"
        } else {
            "MISMATCH/NONE"
        }
    );
    Ok(())
}
