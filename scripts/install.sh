#!/usr/bin/env bash
# Installs GitHub Release Updater on Ubuntu / Debian.
#
#   scripts/install.sh                         build from source, then install the .deb
#   scripts/install.sh --release OWNER/REPO    download the latest released .deb and install it
#   scripts/install.sh --package FILE.deb      install an existing .deb
#
# Options:
#   --skip-deps   do not apt-install build dependencies or rustup
#   -h, --help    show this help
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE=source
REPO=""
PACKAGE=""
SKIP_DEPS=0

BUILD_DEPS=(libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev build-essential curl file pkexec)

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
usage() { sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) MODE=release; REPO="${2:-}"; shift 2 || usage 1 ;;
    --package) MODE=package; PACKAGE="${2:-}"; shift 2 || usage 1 ;;
    --skip-deps) SKIP_DEPS=1; shift ;;
    -h|--help) usage 0 ;;
    *) echo "unknown option: $1" >&2; usage 1 ;;
  esac
done

[[ "$(uname -s)" == Linux ]] || die "this script is for Linux; use scripts/install.ps1 on Windows"
command -v apt-get >/dev/null || die "apt-get not found: only Debian/Ubuntu are supported"
[[ $EUID -ne 0 ]] || die "run as your normal user; sudo is used where needed"

# Installs a .deb with apt so dependencies are resolved. The file is copied to a
# private /tmp dir readable by apt's _apt sandbox user.
install_deb() {
  local deb="$1" tmp
  [[ -f "$deb" ]] || die "package not found: $deb"
  tmp="$(mktemp -d /tmp/gh-release-updater-install.XXXXXX)"
  chmod 755 "$tmp"
  cp "$deb" "$tmp/"
  chmod 644 "$tmp/$(basename "$deb")"
  info "Installing $(basename "$deb") (sudo)"
  sudo apt-get install -y "$tmp/$(basename "$deb")"
  rm -rf "$tmp"
  info "Installed. Start it from the application menu (it will add itself to login items)."
}

build_from_source() {
  cd "$ROOT"
  if [[ $SKIP_DEPS -eq 0 ]]; then
    info "Installing build dependencies (sudo)"
    sudo apt-get update
    sudo apt-get install -y "${BUILD_DEPS[@]}"
    if ! command -v cargo >/dev/null && [[ ! -x "$HOME/.cargo/bin/cargo" ]]; then
      info "Installing Rust via rustup"
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    fi
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
  command -v cargo >/dev/null || die "cargo not found; install Rust from https://rustup.rs"
  command -v node >/dev/null || die "Node.js not found; install Node.js 20+ (e.g. via nvm)"
  local node_major
  node_major="$(node -p 'process.versions.node.split(".")[0]')"
  [[ "$node_major" -ge 20 ]] || die "Node.js 20+ required, found $(node --version)"

  info "Installing npm packages"
  npm ci
  info "Building release package (this takes a few minutes)"
  npx tauri build --bundles deb
  local deb
  deb="$(ls -t "$ROOT"/target/release/bundle/deb/*.deb 2>/dev/null | head -n1)"
  [[ -n "$deb" ]] || die "build finished but no .deb found in target/release/bundle/deb"
  install_deb "$deb"
}

# Downloads the newest .deb for this architecture from a GitHub release and
# verifies it against the SHA-256 digest GitHub publishes for the asset.
install_from_release() {
  [[ "$REPO" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die "--release expects OWNER/REPO"
  command -v curl >/dev/null || die "curl is required"
  command -v python3 >/dev/null || die "python3 is required"
  local arch tmp meta url name digest
  arch="$(dpkg --print-architecture)"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"; trap - RETURN' RETURN
  info "Looking up latest release of $REPO"
  curl -fsSL -H 'Accept: application/vnd.github+json' \
    "https://api.github.com/repos/$REPO/releases/latest" -o "$tmp/release.json" \
    || die "could not read latest release of $REPO"
  meta="$(python3 -I - "$tmp/release.json" "$arch" 2>&1 <<'PY'
import json, re, sys
release = json.load(open(sys.argv[1]))
arch = {"amd64": "x86_64|amd64|x64", "arm64": "aarch64|arm64"}.get(sys.argv[2], sys.argv[2])
pat = re.compile(r"(?i)(^|[^a-z0-9])(%s)([^a-z0-9]|$)" % arch)
debs = [a for a in release.get("assets", []) if a["name"].lower().endswith(".deb")]
pick = [a for a in debs if pat.search(a["name"])] or (debs if len(debs) == 1 else [])
if not pick:
    sys.exit("no .deb asset for %s in %s" % (sys.argv[2], release.get("tag_name")))
a = pick[0]
print(a["browser_download_url"], a["name"], (a.get("digest") or "").removeprefix("sha256:"))
PY
)" || die "$meta"
  read -r url name digest <<<"$meta"
  [[ "$url" == https://github.com/* ]] || die "unexpected download URL: $url"
  info "Downloading $name"
  curl -fL --progress-bar --proto '=https' -o "$tmp/$name" "$url"
  if [[ -n "$digest" ]]; then
    echo "$digest  $tmp/$name" | sha256sum -c --quiet - || die "SHA-256 mismatch for $name"
    info "SHA-256 verified"
  else
    info "No digest published for $name; skipping checksum verification"
  fi
  install_deb "$tmp/$name"
}

case "$MODE" in
  source) build_from_source ;;
  release) install_from_release ;;
  package) install_deb "$PACKAGE" ;;
esac
