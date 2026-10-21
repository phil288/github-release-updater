//! Streams release assets to disk, restricted to GitHub hosts.

use crate::{Error, Result};
use futures_util::StreamExt;
use std::path::Path;
use tokio::io::AsyncWriteExt;

const ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "github-releases.githubusercontent.com",
];

/// Only HTTPS URLs on GitHub's own download hosts are accepted.
pub fn check_url(raw: &str) -> Result<()> {
    let u = url::Url::parse(raw).map_err(|e| Error::DownloadRefused(format!("{raw}: {e}")))?;
    let host = u.host_str().unwrap_or_default();
    if u.scheme() != "https" || !ALLOWED_HOSTS.contains(&host) {
        return Err(Error::DownloadRefused(format!("host not allowed: {raw}")));
    }
    Ok(())
}

/// Builds a client whose redirects may only lead to GitHub download hosts.
pub fn download_client() -> Result<reqwest::Client> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > 10 {
            attempt.error("too many redirects")
        } else if check_url(attempt.url().as_str()).is_err() {
            let u = attempt.url().to_string();
            attempt.error(format!("redirect to disallowed host: {u}"))
        } else {
            attempt.follow()
        }
    });
    Ok(reqwest::Client::builder()
        .user_agent(crate::github::user_agent())
        .redirect(policy)
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// Keeps only characters that are safe in file names on both OSes.
pub fn sanitize_file_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_start_matches('.');
    if s.is_empty() {
        "download".into()
    } else {
        s.to_string()
    }
}

/// Downloads `url` to `dest` (written to `dest.part` first, renamed on success).
/// `progress(downloaded, total)` is called as chunks arrive.
pub async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<()> {
    check_url(url)?;
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let resp = client.get(url).send().await?.error_for_status()?;
    let total = resp.content_length();
    let part = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&part).await?;
    let mut stream = resp.bytes_stream();
    let mut done = 0u64;
    progress(0, total);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    file.flush().await?;
    drop(file);
    if let Some(t) = total {
        if done != t {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::DownloadRefused(format!(
                "incomplete download: {done} of {t} bytes"
            )));
        }
    }
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_allowlist() {
        assert!(check_url("https://github.com/o/r/releases/download/v1/a.deb").is_ok());
        assert!(check_url("https://objects.githubusercontent.com/x").is_ok());
        assert!(check_url("http://github.com/o/r").is_err());
        assert!(check_url("https://evil.com/a.deb").is_err());
        assert!(check_url("https://github.com.evil.com/a.deb").is_err());
    }

    #[test]
    fn sanitizes() {
        assert_eq!(
            sanitize_file_name("Tool Setup 1.0.msi"),
            "Tool_Setup_1.0.msi"
        );
        assert_eq!(sanitize_file_name("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize_file_name("..."), "download");
    }
}
