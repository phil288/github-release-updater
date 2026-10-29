//! Install scripts: some projects publish releases on GitHub but install
//! through a script on their own site (`curl -f https://zed.dev/install.sh | sh`).
//! The script is fetched over HTTPS from the URL the user configured, saved,
//! hashed for the log, and run by [`crate::installer::run_script`].

use crate::{Error, Result};
use std::path::Path;

/// Scripts are small; anything larger is not what the user expects to run.
const MAX_SIZE: usize = 1024 * 1024;

/// Turns what the user typed into the script URL. Accepts a bare URL or a
/// pasted install command such as `curl -fsSL https://zed.dev/install.sh | sh`
/// or `irm https://example.com/install.ps1 | iex`, keeping only the URL.
/// Blank input is `None`.
pub fn normalize(input: &str) -> Result<Option<String>> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(None);
    }
    let url = input
        .split_whitespace()
        .map(|t| t.trim_matches(|c| matches!(c, '"' | '\'' | '(' | ')')))
        .find(|t| t.contains("://"))
        .unwrap_or(input);
    check_url(url)?;
    Ok(Some(url.to_string()))
}

/// Install scripts may live on any host, but only over HTTPS.
pub fn check_url(raw: &str) -> Result<()> {
    if raw.chars().any(char::is_whitespace) {
        return Err(Error::DownloadRefused(format!(
            "install script URL contains spaces: {raw}"
        )));
    }
    let u = url::Url::parse(raw).map_err(|e| Error::DownloadRefused(format!("{raw}: {e}")))?;
    if u.scheme() != "https" || u.host_str().is_none_or(str::is_empty) {
        return Err(Error::DownloadRefused(format!(
            "install script must be an https:// URL: {raw}"
        )));
    }
    Ok(())
}

fn client() -> Result<reqwest::Client> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > 10 {
            attempt.error("too many redirects")
        } else if attempt.url().scheme() != "https" {
            let u = attempt.url().to_string();
            attempt.error(format!("redirect to non-HTTPS URL: {u}"))
        } else {
            attempt.follow()
        }
    });
    Ok(reqwest::Client::builder()
        .user_agent(crate::github::user_agent())
        .redirect(policy)
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(120))
        .build()?)
}

/// Downloads the script at `url` to `dest` and returns its SHA-256.
/// Fails on HTTP errors (like `curl -f`) and on oversized responses.
pub async fn fetch(url: &str, dest: &Path) -> Result<String> {
    check_url(url)?;
    let body = client()?
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if body.len() > MAX_SIZE {
        return Err(Error::DownloadRefused(format!(
            "install script is larger than {MAX_SIZE} bytes: {url}"
        )));
    }
    if body.is_empty() {
        return Err(Error::DownloadRefused(format!(
            "install script is empty: {url}"
        )));
    }
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    tokio::fs::write(dest, &body).await?;
    Ok(crate::checksum::sha256_file(dest).await?)
}

/// File name the script is saved under, so the interpreter accepts it.
pub fn file_name(os: crate::assets::Os) -> &'static str {
    match os {
        crate::assets::Os::Linux => "install.sh",
        crate::assets::Os::Windows => "install.ps1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_only() {
        assert!(check_url("https://zed.dev/install.sh").is_ok());
        assert!(check_url("http://zed.dev/install.sh").is_err());
        assert!(check_url("file:///etc/passwd").is_err());
        assert!(check_url("zed.dev/install.sh").is_err());
        assert!(check_url("https://zed.dev/install.sh | sh").is_err());
    }

    #[test]
    fn extracts_url_from_pasted_command() {
        let n = |s: &str| normalize(s).unwrap();
        let zed = Some("https://zed.dev/install.sh".to_string());
        assert_eq!(n("https://zed.dev/install.sh"), zed);
        assert_eq!(n("https://zed.dev/install.sh | sh"), zed);
        assert_eq!(n("curl -f https://zed.dev/install.sh | sh"), zed);
        assert_eq!(n("curl -fsSL 'https://zed.dev/install.sh' | bash"), zed);
        assert_eq!(
            n("irm https://example.com/install.ps1 | iex"),
            Some("https://example.com/install.ps1".to_string())
        );
        assert_eq!(n("   "), None);
        assert!(normalize("curl -f http://zed.dev/install.sh | sh").is_err());
        assert!(normalize("sh install.sh").is_err());
    }
}
