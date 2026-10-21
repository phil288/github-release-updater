//! Settings backups stored in a Backblaze B2 bucket, through the native B2 API (v4).
//!
//! Every backup is a separate object named
//! `<prefix>gh-release-updater-backup-<UTC timestamp>.json`, so the bucket keeps a
//! browsable history and any of them can be restored. The application key is a
//! secret and is passed in separately: it is never part of [`B2Config`] or a backup.

use crate::config::{write_atomic, Settings};
use crate::{backup, Error, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::path::Path;
use std::time::Duration;

const AUTHORIZE_URL: &str = "https://api.backblazeb2.com/b2api/v4/b2_authorize_account";
const FILE_STEM: &str = "gh-release-updater-backup-";
/// Backups are a few KB; refuse to download anything absurdly large.
const MAX_BACKUP_BYTES: u64 = 10 * 1024 * 1024;

/// Machine-local B2 settings. Kept out of [`Settings`] so that restoring a
/// backup never overwrites or erases the configuration used to fetch it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct B2Config {
    #[serde(default)]
    pub key_id: String,
    #[serde(default)]
    pub bucket: String,
    /// Folder inside the bucket, e.g. `laptop/`. Empty for the bucket root.
    #[serde(default)]
    pub prefix: String,
    /// Upload a backup automatically whenever the settings change.
    #[serde(default)]
    pub auto_backup: bool,
}

impl B2Config {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(serde_json::from_str::<Self>(&text)?.validate()?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, serde_json::to_string_pretty(self)?.as_bytes())
    }

    /// Trims fields, normalises the prefix to `a/b/` form and checks the bucket name.
    /// Empty key ID and bucket are allowed (B2 not configured).
    pub fn validate(mut self) -> Result<Self> {
        self.key_id = self.key_id.trim().to_string();
        self.bucket = self.bucket.trim().to_string();
        let prefix = self.prefix.trim().trim_matches('/');
        self.prefix = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}/")
        };

        if !self.key_id.is_empty() && !self.key_id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(Error::Backblaze(
                "the key ID may only contain letters and digits".into(),
            ));
        }
        if !self.bucket.is_empty()
            && (!(6..=63).contains(&self.bucket.len())
                || !self
                    .bucket
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-'))
        {
            return Err(Error::Backblaze(
                "bucket names are 6 to 63 letters, digits or hyphens".into(),
            ));
        }
        if self.prefix.len() > 512
            || self.prefix.chars().any(|c| c.is_control() || c == '\\')
            || self.prefix.contains("//")
        {
            return Err(Error::Backblaze("invalid folder name".into()));
        }
        Ok(self)
    }

    pub fn is_configured(&self) -> bool {
        !self.key_id.is_empty() && !self.bucket.is_empty()
    }

    /// Object name for a backup taken now.
    pub fn new_object_name(&self) -> String {
        format!(
            "{}{FILE_STEM}{}.json",
            self.prefix,
            chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ")
        )
    }

    fn is_backup_name(&self, name: &str) -> bool {
        name.strip_prefix(self.prefix.as_str())
            .and_then(|rest| rest.strip_prefix(FILE_STEM))
            .is_some_and(|rest| rest.ends_with(".json") && !rest.contains('/'))
    }
}

/// Result of [`B2::test`]. Authentication and bucket problems are errors;
/// missing permissions are reported here so the UI can name them.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TestReport {
    pub bucket: String,
    /// Where new backups go, e.g. `laptop/gh-release-updater-backup-….json`.
    pub location: String,
    pub backups_found: usize,
    /// Capabilities the key lacks: `writeFiles` (back up), `listFiles` (view), `readFiles` (restore).
    pub missing_capabilities: Vec<String>,
    /// Set when the key is restricted to a name prefix the configured folder is outside of.
    pub prefix_problem: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RemoteBackup {
    pub file_id: String,
    pub file_name: String,
    pub size: u64,
    /// Milliseconds since the Unix epoch.
    pub uploaded_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorizeResponse {
    account_id: String,
    authorization_token: String,
    api_info: ApiInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiInfo {
    storage_api: StorageApi,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StorageApi {
    api_url: String,
    download_url: String,
    #[serde(default)]
    allowed: Allowed,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Allowed {
    /// `None` when the key is not restricted to particular buckets.
    #[serde(default)]
    buckets: Option<Vec<AllowedBucket>>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    name_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AllowedBucket {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListBucketsResponse {
    buckets: Vec<BucketInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BucketInfo {
    bucket_id: String,
    bucket_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadUrlResponse {
    upload_url: String,
    authorization_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFilesResponse {
    files: Vec<FileInfo>,
    next_file_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileInfo {
    file_id: Option<String>,
    file_name: String,
    #[serde(default)]
    content_length: u64,
    #[serde(default)]
    upload_timestamp: i64,
    #[serde(default)]
    action: String,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

/// An authorized session bound to the configured bucket.
struct Session {
    api_url: String,
    download_url: String,
    token: String,
    bucket_id: String,
    capabilities: Vec<String>,
    name_prefix: Option<String>,
}

impl Session {
    fn can(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    fn require(&self, capability: &str, action: &str) -> Result<()> {
        if self.can(capability) {
            Ok(())
        } else {
            Err(Error::Backblaze(format!(
                "this application key lacks the {capability} capability needed to {action}"
            )))
        }
    }
}

pub struct B2 {
    http: reqwest::Client,
}

impl B2 {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(crate::github::user_agent())
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self { http })
    }

    /// Checks the key, the bucket and the key's permissions. Writes nothing.
    pub async fn test(&self, cfg: &B2Config, app_key: &str) -> Result<TestReport> {
        let session = self.authorize(cfg, app_key).await?;
        let missing_capabilities: Vec<String> = ["writeFiles", "listFiles", "readFiles"]
            .into_iter()
            .filter(|c| !session.can(c))
            .map(String::from)
            .collect();
        let prefix_problem = session
            .name_prefix
            .as_deref()
            .filter(|p| !cfg.prefix.starts_with(p))
            .map(|p| {
                format!(
                    "the key only allows file names starting with '{p}'; set the folder to match"
                )
            });
        let backups_found = if session.can("listFiles") && prefix_problem.is_none() {
            self.list_with(&session, cfg).await?.len()
        } else {
            0
        };
        Ok(TestReport {
            bucket: cfg.bucket.clone(),
            location: format!("{}{FILE_STEM}<date>.json", cfg.prefix),
            backups_found,
            missing_capabilities,
            prefix_problem,
        })
    }

    /// Uploads a backup of `settings` and returns the object name.
    pub async fn upload(
        &self,
        cfg: &B2Config,
        app_key: &str,
        settings: &Settings,
    ) -> Result<String> {
        let session = self.authorize(cfg, app_key).await?;
        session.require("writeFiles", "upload backups")?;
        let body = backup::to_json(settings)?.into_bytes();
        let name = cfg.new_object_name();

        let target: UploadUrlResponse = self
            .call(
                &session,
                "b2_get_upload_url",
                &serde_json::json!({ "bucketId": session.bucket_id }),
            )
            .await?;
        let sha1 = hex::encode(Sha1::digest(&body));
        let resp = self
            .http
            .post(&target.upload_url)
            .header(AUTHORIZATION, &target.authorization_token)
            .header("X-Bz-File-Name", encode_file_name(&name))
            .header(CONTENT_TYPE, "application/json")
            .header("X-Bz-Content-Sha1", sha1)
            .body(body)
            .send()
            .await?;
        check(resp).await?;
        Ok(name)
    }

    /// Lists the backups in the configured folder, newest first.
    pub async fn list(&self, cfg: &B2Config, app_key: &str) -> Result<Vec<RemoteBackup>> {
        let session = self.authorize(cfg, app_key).await?;
        session.require("listFiles", "list backups")?;
        self.list_with(&session, cfg).await
    }

    /// Downloads one backup and validates it.
    pub async fn download(&self, cfg: &B2Config, app_key: &str, file_id: &str) -> Result<Settings> {
        if file_id.is_empty()
            || !file_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(Error::Backblaze("invalid file ID".into()));
        }
        let session = self.authorize(cfg, app_key).await?;
        session.require("readFiles", "restore backups")?;
        let resp = self
            .http
            .get(format!(
                "{}/b2api/v4/b2_download_file_by_id",
                session.download_url
            ))
            .query(&[("fileId", file_id)])
            .header(AUTHORIZATION, &session.token)
            .send()
            .await?;
        let resp = check(resp).await?;
        if resp.content_length().is_some_and(|n| n > MAX_BACKUP_BYTES) {
            return Err(Error::InvalidBackup(
                "file is too large to be a backup".into(),
            ));
        }
        let text = resp.text().await?;
        backup::parse(&text)
    }

    async fn list_with(&self, session: &Session, cfg: &B2Config) -> Result<Vec<RemoteBackup>> {
        let mut out = Vec::new();
        let mut start: Option<String> = None;
        loop {
            let mut request = serde_json::json!({
                "bucketId": session.bucket_id,
                "prefix": format!("{}{FILE_STEM}", cfg.prefix),
                "delimiter": "/",
                "maxFileCount": 1000,
            });
            if let Some(start) = &start {
                request["startFileName"] = start.as_str().into();
            }
            let page: ListFilesResponse =
                self.call(session, "b2_list_file_names", &request).await?;
            out.extend(page.files.into_iter().filter_map(|f| {
                let file_id = f.file_id?;
                (f.action == "upload" && cfg.is_backup_name(&f.file_name)).then_some(RemoteBackup {
                    file_id,
                    file_name: f.file_name,
                    size: f.content_length,
                    uploaded_at: f.upload_timestamp,
                })
            }));
            match page.next_file_name {
                Some(next) => start = Some(next),
                None => break,
            }
        }
        out.sort_by(|a, b| {
            b.uploaded_at
                .cmp(&a.uploaded_at)
                .then_with(|| b.file_name.cmp(&a.file_name))
        });
        Ok(out)
    }

    async fn authorize(&self, cfg: &B2Config, app_key: &str) -> Result<Session> {
        if !cfg.is_configured() {
            return Err(Error::Backblaze(
                "enter a key ID and a bucket name first".into(),
            ));
        }
        let app_key = app_key.trim();
        if app_key.is_empty() {
            return Err(Error::Backblaze("enter the application key first".into()));
        }
        let resp = self
            .http
            .get(AUTHORIZE_URL)
            .basic_auth(&cfg.key_id, Some(app_key))
            .send()
            .await?;
        let auth: AuthorizeResponse = check(resp).await?.json().await?;
        let storage = auth.api_info.storage_api;
        let mut session = Session {
            api_url: storage.api_url,
            download_url: storage.download_url,
            token: auth.authorization_token,
            bucket_id: String::new(),
            capabilities: storage.allowed.capabilities,
            name_prefix: storage.allowed.name_prefix.filter(|p| !p.is_empty()),
        };
        // A bucket-restricted key names its buckets (the name may be null); otherwise,
        // or when the name is missing, look the bucket up. A restricted key may
        // list the buckets it can access when the name is given.
        let allowed_id = storage
            .allowed
            .buckets
            .iter()
            .flatten()
            .find(|b| b.name.as_deref() == Some(cfg.bucket.as_str()))
            .map(|b| b.id.clone());
        session.bucket_id = match allowed_id {
            Some(id) => id,
            None => {
                let list: ListBucketsResponse = self
                    .call(
                        &session,
                        "b2_list_buckets",
                        &serde_json::json!({ "accountId": auth.account_id, "bucketName": cfg.bucket }),
                    )
                    .await?;
                list.buckets
                    .into_iter()
                    .find(|b| b.bucket_name == cfg.bucket)
                    .map(|b| b.bucket_id)
                    .ok_or_else(|| {
                        Error::Backblaze(format!(
                            "bucket '{}' not found, or this application key has no access to it",
                            cfg.bucket
                        ))
                    })?
            }
        };
        Ok(session)
    }

    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        session: &Session,
        op: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let resp = self
            .http
            .post(format!("{}/b2api/v4/{op}", session.api_url))
            .header(AUTHORIZATION, &session.token)
            .json(body)
            .send()
            .await?;
        Ok(check(resp).await?.json().await?)
    }
}

/// Turns B2's JSON error bodies into readable errors.
async fn check(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let text = resp.text().await.unwrap_or_default();
    let api: Option<ApiError> = serde_json::from_str(&text).ok();
    Err(Error::Backblaze(describe_error(status.as_u16(), api)))
}

fn describe_error(status: u16, api: Option<ApiError>) -> String {
    let (code, message) = api.map(|e| (e.code, e.message)).unwrap_or_default();
    match (status, code.as_str()) {
        (401, "bad_auth_token" | "unauthorized" | "") => {
            "the key ID or application key was rejected".to_string()
        }
        (401, _) | (403, _) => format!("access denied: {message}"),
        (404, _) => "not found (the backup may have been deleted)".to_string(),
        _ if message.is_empty() => format!("HTTP {status}"),
        _ => format!("{message} ({code})"),
    }
}

/// Percent-encodes a file name for the `X-Bz-File-Name` header (UTF-8, `/` kept).
fn encode_file_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(prefix: &str) -> B2Config {
        B2Config {
            key_id: "0012ab".into(),
            bucket: "my-backups".into(),
            prefix: prefix.into(),
            auto_backup: false,
        }
        .validate()
        .unwrap()
    }

    #[test]
    fn prefix_is_normalised() {
        assert_eq!(cfg("").prefix, "");
        assert_eq!(cfg("  /laptop/ ").prefix, "laptop/");
        assert_eq!(cfg("a/b").prefix, "a/b/");
    }

    #[test]
    fn rejects_bad_config() {
        let base = cfg("");
        let bad = |f: fn(&mut B2Config)| {
            let mut c = base.clone();
            f(&mut c);
            c.validate().is_err()
        };
        assert!(bad(|c| c.bucket = "abc".into()), "too short");
        assert!(bad(|c| c.bucket = "my_bucket".into()), "underscore");
        assert!(bad(|c| c.key_id = "key id".into()), "space");
        assert!(bad(|c| c.prefix = "a//b".into()), "double slash");
        assert!(bad(|c| c.prefix = "a\nb".into()), "control char");
        assert!(B2Config::default().validate().is_ok(), "empty is allowed");
        assert!(!B2Config::default().is_configured());
    }

    #[test]
    fn object_names() {
        let c = cfg("laptop");
        let name = c.new_object_name();
        assert!(
            name.starts_with("laptop/gh-release-updater-backup-20"),
            "{name}"
        );
        assert!(name.ends_with("Z.json"));
        assert!(c.is_backup_name(&name));
        assert!(!c.is_backup_name("laptop/other.json"));
        assert!(!c.is_backup_name("laptop/gh-release-updater-backup-x/y.json"));
        assert!(!c.is_backup_name("desktop/gh-release-updater-backup-1.json"));
        assert!(cfg("").is_backup_name("gh-release-updater-backup-1.json"));
    }

    #[test]
    fn encodes_file_names() {
        assert_eq!(encode_file_name("a b/c+d.json"), "a%20b/c%2Bd.json");
        assert_eq!(encode_file_name("é"), "%C3%A9");
    }

    #[test]
    fn parses_authorize_response() {
        let restricted = r#"{"accountId":"acc","authorizationToken":"tok","apiInfo":{"storageApi":{
            "apiUrl":"https://api005.backblazeb2.com","downloadUrl":"https://f005.backblazeb2.com",
            "allowed":{"buckets":[{"id":"b1","name":"my-backups"}],"capabilities":["listFiles","readFiles"],"namePrefix":null}}}}"#;
        let a: AuthorizeResponse = serde_json::from_str(restricted).unwrap();
        let allowed = a.api_info.storage_api.allowed;
        assert_eq!(allowed.buckets.unwrap()[0].id, "b1");
        assert_eq!(allowed.capabilities, ["listFiles", "readFiles"]);

        let unrestricted = r#"{"accountId":"acc","authorizationToken":"tok","apiInfo":{"storageApi":{
            "apiUrl":"u","downloadUrl":"d","allowed":{"buckets":null,"capabilities":["writeFiles"]}}}}"#;
        let a: AuthorizeResponse = serde_json::from_str(unrestricted).unwrap();
        assert!(a.api_info.storage_api.allowed.buckets.is_none());
    }

    #[test]
    fn error_messages() {
        let e = |status, code: &str, message: &str| {
            describe_error(
                status,
                Some(ApiError {
                    code: code.into(),
                    message: message.into(),
                }),
            )
        };
        assert_eq!(
            e(401, "bad_auth_token", "x"),
            "the key ID or application key was rejected"
        );
        assert_eq!(
            e(401, "unauthorized", ""),
            "the key ID or application key was rejected"
        );
        assert_eq!(
            e(400, "bad_request", "Invalid bucketId"),
            "Invalid bucketId (bad_request)"
        );
        assert_eq!(describe_error(503, None), "HTTP 503");
    }
}
