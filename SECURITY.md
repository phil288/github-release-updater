# Security policy

GitHub Release Updater downloads installers from the internet and runs them with
administrator rights, so security reports are taken seriously.

## Reporting a vulnerability

Report privately through
[GitHub Security Advisories](https://github.com/phil288/github-release-updater/security/advisories/new).
Do not open a public issue. Include the version, OS, and steps to reproduce.

You can expect a first answer within 7 days. Fixes ship in a new release, and the
advisory is published once users can update.

## Supported versions

Only the latest release receives security fixes.

## Scope

In scope, for example:

* Installing a file that does not come from the tracked repository's GitHub release
  (download host allow-list, redirect handling, file naming).
* Bypassing SHA-256 verification when a checksum is published.
* Leaking the GitHub token or the Backblaze B2 application key (they must never appear
  in backups, logs or the UI).
* Privilege escalation through the `pkexec` / `msiexec` install and uninstall paths.

Out of scope: a tracked project that publishes a malicious release. The app installs
what the repository owner publishes; only track repositories you trust.

## How installs are protected

* HTTPS only; downloads and redirects restricted to GitHub hosts.
* SHA-256 verified from the GitHub asset digest, `<asset>.sha256` or `SHA256SUMS*` when
  available. Enable *Only install releases that publish a SHA-256 checksum* to refuse
  releases without one.
* Every install and uninstall needs your approval in the window, then the OS's own
  elevation prompt (polkit password on Ubuntu, UAC on Windows).
* Release packages of this app are built by GitHub Actions on GitHub-hosted runners and
  published with `SHA256SUMS.txt`.
