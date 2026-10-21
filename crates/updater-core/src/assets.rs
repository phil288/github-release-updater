//! Picks the release asset that fits the current OS and CPU architecture.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
    /// `sha256:<hex>` as published by the GitHub API (newer releases only).
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Linux,
    Windows,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    X64,
    Arm64,
    X86,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

impl Platform {
    pub fn current() -> Self {
        let os = if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        };
        let arch = match std::env::consts::ARCH {
            "x86_64" => Arch::X64,
            "aarch64" => Arch::Arm64,
            "x86" => Arch::X86,
            _ => Arch::Other,
        };
        Platform { os, arch }
    }

    /// File extension of the installer type handled on this OS.
    pub fn extension(&self) -> &'static str {
        match self.os {
            Os::Linux => ".deb",
            Os::Windows => ".msi",
        }
    }
}

fn word(alts: &str) -> Regex {
    Regex::new(&format!("(?i)(?:^|[^a-z0-9])(?:{alts})(?:[^a-z0-9]|$)")).unwrap()
}

static X64: LazyLock<Regex> = LazyLock::new(|| word("x86_64|x86-64|amd64|x64|win64"));
static ARM64: LazyLock<Regex> = LazyLock::new(|| word("aarch64|arm64"));
static X86: LazyLock<Regex> = LazyLock::new(|| word("x86|i386|i686|win32|ia32"));
static OTHER: LazyLock<Regex> = LazyLock::new(|| {
    word("riscv64|armhf|armel|armv7l?|armv6l?|ppc64el|ppc64le|s390x|loong64|mips64el")
});
static PENALTY: LazyLock<Regex> =
    LazyLock::new(|| word("cli|web|portable|debug|dbg|dbgsym|symbols|src|source"));

/// Architecture named in an asset file name, if any.
pub fn arch_of(name: &str) -> Option<Arch> {
    // Order matters: "x86_64" must win over "x86".
    if X64.is_match(name) {
        Some(Arch::X64)
    } else if ARM64.is_match(name) {
        Some(Arch::Arm64)
    } else if OTHER.is_match(name) {
        Some(Arch::Other)
    } else if X86.is_match(name) {
        Some(Arch::X86)
    } else {
        None
    }
}

/// Selects the installer asset for `platform`.
///
/// With a user `pattern`, the first asset with the right extension whose name
/// matches it wins. Otherwise assets for another architecture are dropped,
/// an explicit architecture match beats an arch-less name, and variants such
/// as `-cli` or `-portable` are de-prioritised.
pub fn select<'a>(
    assets: &'a [Asset],
    platform: Platform,
    pattern: Option<&Regex>,
) -> Option<&'a Asset> {
    let ext = platform.extension();
    let candidates = assets
        .iter()
        .filter(|a| a.name.to_ascii_lowercase().ends_with(ext));
    if let Some(re) = pattern {
        return candidates.into_iter().find(|a| re.is_match(&a.name));
    }
    candidates
        .filter_map(|a| {
            let mut score = match arch_of(&a.name) {
                Some(arch) if arch == platform.arch => 10,
                Some(_) => return None,
                None => 1,
            };
            if PENALTY.is_match(&a.name) {
                score -= 5;
            }
            Some((score, a))
        })
        // Highest score, then shortest name (fewest extra qualifiers).
        .max_by(|(sa, a), (sb, b)| sa.cmp(sb).then(b.name.len().cmp(&a.name.len())))
        .map(|(_, a)| a)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn wordcraft_assets() -> Vec<Asset> {
        [
            "SHA256SUMS.txt",
            "wordcraft-0.8.0-freebsd-x86_64.tar.gz",
            "wordcraft-0.8.0-linux-aarch64.AppImage",
            "wordcraft-0.8.0-linux-aarch64.deb",
            "wordcraft-0.8.0-linux-aarch64.rpm",
            "wordcraft-0.8.0-linux-riscv64.deb",
            "wordcraft-0.8.0-linux-x86_64.AppImage",
            "wordcraft-0.8.0-linux-x86_64.deb",
            "wordcraft-0.8.0-linux-x86_64.rpm",
            "wordcraft-0.8.0-macos-universal.dmg",
            "wordcraft-0.8.0-windows-arm64-portable.zip",
            "wordcraft-0.8.0-windows-arm64.msi",
            "wordcraft-0.8.0-windows-x64-portable.zip",
            "wordcraft-0.8.0-windows-x64.msi",
            "wordcraft-0.8.0-windows-x86.msi",
            "wordcraft-cli-0.8.0-macos-universal.zip",
            "wordcraft-web-0.8.0.zip",
        ]
        .iter()
        .map(|n| Asset {
            name: n.to_string(),
            browser_download_url: format!(
                "https://github.com/storytold/wordcraft/releases/download/v0.8.0/{n}"
            ),
            size: 1,
            digest: None,
        })
        .collect()
    }

    fn pick(os: Os, arch: Arch, assets: &[Asset]) -> Option<String> {
        select(assets, Platform { os, arch }, None).map(|a| a.name.clone())
    }

    #[test]
    fn wordcraft() {
        let a = wordcraft_assets();
        assert_eq!(
            pick(Os::Linux, Arch::X64, &a).unwrap(),
            "wordcraft-0.8.0-linux-x86_64.deb"
        );
        assert_eq!(
            pick(Os::Linux, Arch::Arm64, &a).unwrap(),
            "wordcraft-0.8.0-linux-aarch64.deb"
        );
        assert_eq!(pick(Os::Linux, Arch::X86, &a), None);
        assert_eq!(
            pick(Os::Windows, Arch::X64, &a).unwrap(),
            "wordcraft-0.8.0-windows-x64.msi"
        );
        assert_eq!(
            pick(Os::Windows, Arch::Arm64, &a).unwrap(),
            "wordcraft-0.8.0-windows-arm64.msi"
        );
        assert_eq!(
            pick(Os::Windows, Arch::X86, &a).unwrap(),
            "wordcraft-0.8.0-windows-x86.msi"
        );
    }

    #[test]
    fn debian_naming_and_archless() {
        let mk = |names: &[&str]| -> Vec<Asset> {
            names
                .iter()
                .map(|n| Asset {
                    name: n.to_string(),
                    browser_download_url: String::new(),
                    size: 0,
                    digest: None,
                })
                .collect()
        };
        let a = mk(&[
            "tool_1.0_amd64.deb",
            "tool_1.0_arm64.deb",
            "tool_1.0_armhf.deb",
        ]);
        assert_eq!(
            pick(Os::Linux, Arch::X64, &a).unwrap(),
            "tool_1.0_amd64.deb"
        );
        let a = mk(&["tool.deb", "tool-cli.deb"]);
        assert_eq!(pick(Os::Linux, Arch::X64, &a).unwrap(), "tool.deb");
        let a = mk(&["Tool Setup 1.0.MSI"]);
        assert_eq!(
            pick(Os::Windows, Arch::X64, &a).unwrap(),
            "Tool Setup 1.0.MSI"
        );
    }

    #[test]
    fn user_pattern() {
        let a = wordcraft_assets();
        let re = Regex::new("windows-x86\\.msi$").unwrap();
        let p = Platform {
            os: Os::Windows,
            arch: Arch::X64,
        };
        assert_eq!(
            select(&a, p, Some(&re)).unwrap().name,
            "wordcraft-0.8.0-windows-x86.msi"
        );
    }

    #[test]
    fn arch_detection() {
        assert_eq!(arch_of("a-x86_64.deb"), Some(Arch::X64));
        assert_eq!(arch_of("a-x86.msi"), Some(Arch::X86));
        assert_eq!(arch_of("a_i386.deb"), Some(Arch::X86));
        assert_eq!(arch_of("a-riscv64.deb"), Some(Arch::Other));
        assert_eq!(arch_of("a.deb"), None);
    }
}
