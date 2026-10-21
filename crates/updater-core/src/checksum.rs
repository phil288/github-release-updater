//! SHA-256 helpers: hashing files and reading published checksum lists.

use crate::assets::Asset;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use tokio::io::AsyncReadExt;

pub async fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Parses GNU (`<hex>  name`, `<hex> *name`) and BSD (`SHA256 (name) = <hex>`)
/// checksum lists into `file name -> lowercase hex`.
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        if let Some(rest) = line.strip_prefix("SHA256 (") {
            if let Some((name, hash)) = rest.split_once(") = ") {
                if is_hex64(hash.trim()) {
                    out.insert(name.to_string(), hash.trim().to_ascii_lowercase());
                }
            }
            continue;
        }
        if let Some((hash, name)) = line.split_once(char::is_whitespace) {
            let name = name.trim_start().trim_start_matches('*');
            // Some lists prefix names with "./" or a directory.
            let name = name.rsplit('/').next().unwrap_or(name);
            if is_hex64(hash) && !name.is_empty() {
                out.insert(name.to_string(), hash.to_ascii_lowercase());
            }
        }
    }
    out
}

/// Where the expected checksum for an asset comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source<'a> {
    /// GitHub API `digest` field.
    Digest(String),
    /// A sibling asset (`<name>.sha256`) or a checksum list (`SHA256SUMS*`).
    File(&'a Asset),
}

/// Finds the best checksum source for `target` among the release assets.
pub fn find_source<'a>(target: &Asset, assets: &'a [Asset]) -> Option<Source<'a>> {
    if let Some(hex) = target
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|h| is_hex64(h))
    {
        return Some(Source::Digest(hex.to_ascii_lowercase()));
    }
    let lower = target.name.to_ascii_lowercase();
    let sibling = assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        n == format!("{lower}.sha256") || n == format!("{lower}.sha256sum")
    });
    if let Some(a) = sibling {
        return Some(Source::File(a));
    }
    assets
        .iter()
        .find(|a| {
            let n = a.name.to_ascii_lowercase();
            n.starts_with("sha256sum")
                || (n.contains("checksum") && !n.ends_with(".sig") && !n.ends_with(".asc"))
        })
        .map(Source::File)
}

/// Extracts the hash for `target_name` from the content of a checksum file.
/// A single-entry file without a usable name (`<hex>` only) is accepted too.
pub fn expected_from_file(content: &str, target_name: &str) -> Option<String> {
    let sums = parse_sums(content);
    if let Some(h) = sums.get(target_name) {
        return Some(h.clone());
    }
    let first = content.split_whitespace().next()?;
    (sums.len() <= 1 && is_hex64(first)).then(|| first.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "a3f1c4b2e5d6978812345678901234567890abcdefabcdefabcdefabcdef0123";

    #[test]
    fn parses_formats() {
        let text = format!("{H}  wordcraft-0.8.0-linux-x86_64.deb\n{H} *./dist/x.msi\nSHA256 (y.deb) = {H}\n# comment\n");
        let s = parse_sums(&text);
        assert_eq!(s.get("wordcraft-0.8.0-linux-x86_64.deb").unwrap(), H);
        assert_eq!(s.get("x.msi").unwrap(), H);
        assert_eq!(s.get("y.deb").unwrap(), H);
    }

    #[test]
    fn single_hash_file() {
        assert_eq!(expected_from_file(&format!("{H}\n"), "a.deb").unwrap(), H);
        assert_eq!(
            expected_from_file(&format!("{H}  other.deb\n{H}  more.deb"), "a.deb"),
            None
        );
    }

    #[test]
    fn finds_sources() {
        let assets = crate::assets::tests::wordcraft_assets();
        let deb = assets
            .iter()
            .find(|a| a.name.ends_with("linux-x86_64.deb"))
            .unwrap();
        match find_source(deb, &assets) {
            Some(Source::File(a)) => assert_eq!(a.name, "SHA256SUMS.txt"),
            other => panic!("{other:?}"),
        }
        let mut with_digest = deb.clone();
        with_digest.digest = Some(format!("sha256:{H}"));
        assert_eq!(
            find_source(&with_digest, &assets),
            Some(Source::Digest(H.to_string()))
        );
    }

    #[tokio::test]
    async fn hashes_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256_file(&p).await.unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
