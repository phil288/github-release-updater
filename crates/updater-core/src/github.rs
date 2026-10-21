//! Minimal GitHub REST client for release lookups.

use crate::assets::Asset;
use crate::{version, Error, Result};
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH, USER_AGENT,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

const API: &str = "https://api.github.com";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    pub html_url: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

pub struct Client {
    http: reqwest::Client,
    token: Mutex<Option<String>>,
    /// url -> (etag, body). A 304 answer does not count against the rate limit.
    cache: Mutex<HashMap<String, (String, String)>>,
}

pub fn user_agent() -> String {
    format!("gh-release-updater/{}", env!("CARGO_PKG_VERSION"))
}

impl Client {
    pub fn new(token: Option<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent())
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            token: Mutex::new(token),
            cache: Mutex::new(HashMap::new()),
        })
    }

    pub fn set_token(&self, token: Option<String>) {
        *self.token.lock().unwrap() = token.filter(|t| !t.trim().is_empty());
        self.cache.lock().unwrap().clear();
    }

    fn headers(&self, url: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        h.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );
        h.insert(USER_AGENT, HeaderValue::from_str(&user_agent()).unwrap());
        if let Some(t) = self.token.lock().unwrap().as_deref() {
            if let Ok(v) = HeaderValue::from_str(&format!("Bearer {t}")) {
                h.insert(AUTHORIZATION, v);
            }
        }
        if let Some((etag, _)) = self.cache.lock().unwrap().get(url) {
            if let Ok(v) = HeaderValue::from_str(etag) {
                h.insert(IF_NONE_MATCH, v);
            }
        }
        h
    }

    /// GET returning the body, or `None` on 404.
    async fn get_json(&self, repo: &str, url: &str) -> Result<Option<String>> {
        let resp = self.http.get(url).headers(self.headers(url)).send().await?;
        let status = resp.status();
        if status == StatusCode::NOT_MODIFIED {
            if let Some((_, body)) = self.cache.lock().unwrap().get(url) {
                return Ok(Some(body.clone()));
            }
        }
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            let remaining = resp
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let reset = resp
                .headers()
                .get("x-ratelimit-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<i64>().ok())
                .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                .map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string());
            if remaining.as_deref() == Some("0") {
                return Err(Error::GitHub {
                    repo: repo.into(),
                    message: format!(
                        "API rate limit reached, resets at {}. Add a GitHub token in Settings to raise the limit.",
                        reset.unwrap_or_else(|| "?".into())
                    ),
                });
            }
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_owned))
                .unwrap_or(text);
            return Err(Error::GitHub {
                repo: repo.into(),
                message: format!("{status}: {msg}"),
            });
        }
        let etag = resp
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let body = resp.text().await?;
        if let Some(etag) = etag {
            self.cache
                .lock()
                .unwrap()
                .insert(url.to_string(), (etag, body.clone()));
        }
        Ok(Some(body))
    }

    /// The most recent releases of `repo` (newest first, up to 50), drafts included.
    /// An empty list means the repository has no release yet.
    ///
    /// The list endpoint is used instead of `/releases/latest`, because GitHub's
    /// "latest" is the most recently *published* release, which can be a
    /// backport (v0.7.5 published after v0.8.0) rather than the highest version.
    pub async fn releases(&self, repo: &str) -> Result<Vec<Release>> {
        let url = format!("{API}/repos/{repo}/releases?per_page=50");
        let Some(body) = self.get_json(repo, &url).await? else {
            return Err(Error::GitHub {
                repo: repo.into(),
                message: "repository not found (or private without a token)".into(),
            });
        };
        Ok(serde_json::from_str(&body)?)
    }

    /// Downloads a small text asset (checksum list).
    pub async fn fetch_text(&self, url: &str) -> Result<String> {
        crate::download::check_url(url)?;
        let resp = self.http.get(url).send().await?.error_for_status()?;
        if resp.content_length().unwrap_or(0) > 1024 * 1024 {
            return Err(Error::DownloadRefused(format!(
                "checksum file too large: {url}"
            )));
        }
        Ok(resp.text().await?)
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }
}

/// Whether a release may be offered: never drafts; pre-releases only when asked.
/// A tag with a semver pre-release part (`v1.0.0-rc.1`) counts as a pre-release
/// even if the GitHub flag is not set.
pub fn eligible(release: &Release, include_prerelease: bool) -> bool {
    let semver_pre = version::parse_tag(&release.tag_name).is_some_and(|v| !v.pre.is_empty());
    !release.draft && (include_prerelease || !(release.prerelease || semver_pre))
}

/// The highest-versioned eligible release.
pub fn pick_highest(releases: &[Release], include_prerelease: bool) -> Option<&Release> {
    releases
        .iter()
        .filter(|r| eligible(r, include_prerelease))
        .max_by(|a, b| version::compare(&a.tag_name, &b.tag_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_and_picks() {
        let json = r#"[
          {"tag_name":"v0.9.0-beta.1","prerelease":true,"draft":false,"html_url":"u","assets":[{"name":"a.deb","browser_download_url":"https://github.com/x","size":3,"digest":"sha256:00"}]},
          {"tag_name":"v1.0.0","prerelease":false,"draft":true,"html_url":"u","assets":[]},
          {"tag_name":"v0.8.0","prerelease":false,"draft":false,"html_url":"u","assets":[]}
        ]"#;
        let rs: Vec<Release> = serde_json::from_str(json).unwrap();
        assert_eq!(rs[0].assets[0].digest.as_deref(), Some("sha256:00"));
        assert_eq!(pick_highest(&rs, true).unwrap().tag_name, "v0.9.0-beta.1");
        assert_eq!(pick_highest(&rs, false).unwrap().tag_name, "v0.8.0");
    }

    #[test]
    fn highest_version_beats_latest_published() {
        let json = r#"[
          {"tag_name":"v0.7.5","html_url":"u"},
          {"tag_name":"v0.8.0","html_url":"u"},
          {"tag_name":"v0.9.0-rc.1","html_url":"u"},
          {"tag_name":"v0.7.0","html_url":"u"}
        ]"#;
        let rs: Vec<Release> = serde_json::from_str(json).unwrap();
        // v0.9.0-rc.1 has no GitHub prerelease flag but a semver pre-release tag.
        assert_eq!(pick_highest(&rs, false).unwrap().tag_name, "v0.8.0");
        assert_eq!(pick_highest(&rs, true).unwrap().tag_name, "v0.9.0-rc.1");
    }
}
