//! Normalises user input (`owner/name` or any github.com URL) to `owner/name`.

use crate::{Error, Result};

fn valid_part(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.len() <= 100
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Accepts `owner/name`, `github.com/owner/name`, `https://github.com/owner/name/releases/tag/v1`,
/// `git@github.com:owner/name.git` and returns `owner/name`.
pub fn parse_repo(input: &str) -> Result<String> {
    let err = || Error::InvalidRepo(input.to_string());
    let mut s = input.trim();
    for prefix in ["https://", "http://"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
        }
    }
    if let Some(rest) = s.strip_prefix("git@github.com:") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("www.github.com/") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("github.com/") {
        s = rest;
    } else if s.contains("://")
        || s.split('/')
            .next()
            .is_some_and(|h| h.contains('.') && s.matches('/').count() > 1)
    {
        // Some other host.
        return Err(err());
    }
    let mut parts = s.split(['/', '?', '#']).filter(|p| !p.is_empty());
    let owner = parts.next().ok_or_else(err)?;
    let name = parts.next().ok_or_else(err)?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    if !valid_part(owner) || !valid_part(name) {
        return Err(err());
    }
    Ok(format!("{owner}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_variants() {
        for input in [
            "storytold/wordcraft",
            " storytold/wordcraft ",
            "https://github.com/storytold/wordcraft",
            "https://github.com/storytold/wordcraft/releases/tag/v0.8.0",
            "github.com/storytold/wordcraft/",
            "git@github.com:storytold/wordcraft.git",
            "https://github.com/storytold/wordcraft?tab=readme",
        ] {
            assert_eq!(parse_repo(input).unwrap(), "storytold/wordcraft", "{input}");
        }
    }

    #[test]
    fn rejects_garbage() {
        for input in [
            "",
            "wordcraft",
            "https://gitlab.com/a/b",
            "a/b c",
            "../x",
            "a/..",
            "owner/na;me",
        ] {
            assert!(parse_repo(input).is_err(), "{input}");
        }
    }
}
