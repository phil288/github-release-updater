//! Lenient version parsing for release tags and installed-version strings.

use semver::Version;
use std::cmp::Ordering;

/// Parses a release tag such as `v0.8.0`, `release-1.2`, `1.2.3-beta.1` into semver.
/// Missing minor/patch components are filled with zero; a 4th numeric component
/// (Windows `1.2.3.0`) is dropped.
pub fn parse_tag(tag: &str) -> Option<Version> {
    let t = tag.trim();
    let start = t.find(|c: char| c.is_ascii_digit())?;
    let t = &t[start..];
    if let Ok(v) = Version::parse(t) {
        return Some(v);
    }
    // Split numeric core from pre-release/build suffix.
    let core_end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (core, rest) = t.split_at(core_end);
    let nums: Vec<u64> = core
        .split('.')
        .filter(|p| !p.is_empty())
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    if nums.is_empty() {
        return None;
    }
    let get = |i: usize| nums.get(i).copied().unwrap_or(0);
    let mut v = Version::new(get(0), get(1), get(2));
    let rest = rest.trim_start_matches(['-', '+', '_', '.']);
    if !rest.is_empty() {
        let pre: String = rest
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' {
                    c
                } else {
                    '.'
                }
            })
            .collect();
        v.pre = semver::Prerelease::new(pre.trim_matches('.')).unwrap_or_default();
    }
    Some(v)
}

/// Normalises a version reported by dpkg (`1:0.8.0-1ubuntu2`) or the Windows
/// registry (`0.8.0.0`) so it compares equal to the upstream tag `v0.8.0`.
pub fn normalize_installed(raw: &str) -> String {
    let mut s = raw.trim();
    // Debian epoch.
    if let Some((epoch, rest)) = s.split_once(':') {
        if epoch.chars().all(|c| c.is_ascii_digit()) {
            s = rest;
        }
    }
    // Debian revision: last '-' followed by something that starts with a digit
    // (e.g. "-1", "-1ubuntu2"). Upstream pre-releases ("-beta.1") are kept.
    if let Some(idx) = s.rfind('-') {
        if s[idx + 1..].starts_with(|c: char| c.is_ascii_digit()) {
            s = &s[..idx];
        }
    }
    // Windows 4-part version with trailing ".0".
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 4 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        return parts[..3].join(".");
    }
    s.to_string()
}

/// Compares two version strings. Falls back to plain string comparison when
/// either side does not parse.
pub fn compare(a: &str, b: &str) -> Ordering {
    match (parse_tag(a), parse_tag(b)) {
        (Some(x), Some(y)) => x.cmp_precedence(&y),
        _ => a
            .trim_start_matches(['v', 'V'])
            .cmp(b.trim_start_matches(['v', 'V'])),
    }
}

/// True when `candidate` should be offered as an update over `installed`.
pub fn is_newer(candidate: &str, installed: Option<&str>) -> bool {
    match installed {
        None => true,
        Some(i) if i.trim().is_empty() => true,
        Some(i) => compare(candidate, &normalize_installed(i)) == Ordering::Greater,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags() {
        assert_eq!(parse_tag("v0.8.0"), Some(Version::new(0, 8, 0)));
        assert_eq!(parse_tag("release-1.2"), Some(Version::new(1, 2, 0)));
        assert_eq!(parse_tag("2024.10"), Some(Version::new(2024, 10, 0)));
        assert_eq!(parse_tag("1.2.3.4"), Some(Version::new(1, 2, 3)));
        assert_eq!(parse_tag("v1.0.0-beta.2").unwrap().pre.as_str(), "beta.2");
        assert_eq!(parse_tag("v1.0.0_rc1").unwrap().pre.as_str(), "rc1");
        assert_eq!(parse_tag("nightly"), None);
    }

    #[test]
    fn newer() {
        assert!(is_newer("v0.8.0", Some("0.7.10")));
        assert!(is_newer("v0.10.0", Some("0.9.0")));
        assert!(!is_newer("v0.8.0", Some("0.8.0")));
        assert!(!is_newer("v0.8.0", Some("1:0.8.0-1ubuntu1")));
        assert!(!is_newer("v0.8.0", Some("0.8.0.0")));
        assert!(is_newer("v1.0.0", Some("1.0.0-rc.1")));
        assert!(!is_newer("v1.0.0-rc.1", Some("1.0.0")));
        assert!(is_newer("v0.1.0", None));
    }

    #[test]
    fn normalizes() {
        assert_eq!(normalize_installed("1:2.3.4-0ubuntu1"), "2.3.4");
        assert_eq!(normalize_installed("2.3.4-beta.1"), "2.3.4-beta.1");
        assert_eq!(normalize_installed("1.2.3.0"), "1.2.3");
    }
}
