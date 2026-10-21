#!/usr/bin/env bash
# Uninstalls GitHub Release Updater from Ubuntu / Debian.
#
#   scripts/uninstall.sh           remove the package and your login item, keep settings
#   scripts/uninstall.sh --purge   also delete settings, token, downloads and logs
#
# Options:
#   -y, --yes     do not ask for confirmation
#   -h, --help    show this help
set -euo pipefail

APP_NAME="GitHub Release Updater"
APP_ID="dev.phil.gh-release-updater"
BINARY="gh-release-updater"
PURGE=0
ASSUME_YES=0

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
usage() { sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --purge) PURGE=1; shift ;;
    -y|--yes) ASSUME_YES=1; shift ;;
    -h|--help) usage 0 ;;
    *) echo "unknown option: $1" >&2; usage 1 ;;
  esac
done

[[ "$(uname -s)" == Linux ]] || die "this script is for Linux; use scripts/uninstall.ps1 on Windows"
[[ $EUID -ne 0 ]] || die "run as your normal user (per-user files live in your home); sudo is used where needed"

# The Debian package that owns the installed binary; empty when not installed.
package=""
if [[ -e "/usr/bin/$BINARY" ]]; then
  package="$(dpkg -S "/usr/bin/$BINARY" 2>/dev/null | cut -d: -f1 | head -n1 || true)"
fi

user_paths=(
  "${XDG_CONFIG_HOME:-$HOME/.config}/$APP_ID"
  "${XDG_CACHE_HOME:-$HOME/.cache}/$APP_ID"
  "${XDG_DATA_HOME:-$HOME/.local/share}/$APP_ID"
)
autostart="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/$APP_NAME.desktop"

echo "This will:"
[[ -n "$package" ]] && echo "  - remove package '$package' (sudo)" || echo "  - (package not installed)"
echo "  - stop the running app and remove your login item"
if [[ $PURGE -eq 1 ]]; then
  for p in "${user_paths[@]}"; do echo "  - delete $p"; done
fi
if [[ $ASSUME_YES -eq 0 ]]; then
  read -r -p "Continue? [y/N] " answer
  [[ "$answer" =~ ^[Yy]$ ]] || { echo "Aborted."; exit 1; }
fi

# pgrep -x would miss it: process names are truncated to 15 characters.
if pgrep -f "^/usr/bin/$BINARY( |$)" >/dev/null; then
  info "Stopping running instance"
  pkill -f "^/usr/bin/$BINARY( |$)" || true
fi

if [[ -n "$package" ]]; then
  info "Removing package $package (sudo)"
  sudo apt-get remove -y "$package"
fi

# The login item is per user and not owned by the package.
if [[ -f "$autostart" ]]; then
  info "Removing login item"
  rm -f "$autostart"
fi

if [[ $PURGE -eq 1 ]]; then
  for p in "${user_paths[@]}"; do
    if [[ -e "$p" ]]; then
      info "Deleting $p"
      rm -rf -- "$p"
    fi
  done
else
  info "Settings kept in ${user_paths[0]} (use --purge to delete)"
fi
info "Done."
