# Public release preparation (docs, SEO, marketing)

- **Date:** 2026-10-20
- **Branch:** `feat/initial-app`
- **Status:** done in the repository; GitHub settings and posting left to the maintainer

## Context
The repository (`phil288/github-release-updater`) is about to go public. It had a
developer-oriented README, no LICENSE file (MIT only declared in `Cargo.toml`), no community
files, no screenshots, and CI/release on self-hosted runners.

## Decisions
- **CI and release moved to GitHub-hosted runners** (`ubuntu-22.04`, `windows-latest`).
  Self-hosted runners on a public repository are a remote-code-execution risk: for
  `pull_request` events, GitHub runs the workflow file from the PR, so a fork can delete the
  `if: head.repo == repository` guard or add its own workflow targeting `self-hosted`.
  Hosted runners are free for public repos. The `.deb` is built on 22.04 for an older glibc
  baseline (installs on 22.04+). Added `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, npm cache.
- **Release body** now carries an install table and a checksum-verify command.
- **README rewritten for users first** (tagline, badges, screenshots, features, install, usage,
  FAQ). Developer content (source install scripts, build, release, data locations) moved to
  `docs/development.md` rather than deleted.
- **Screenshots from a mock harness** (`scripts/screenshots/`): the real Svelte UI in headless
  Chrome with `@tauri-apps/api/mocks` `mockIPC` and demo data (draw.io, LocalSend, ripgrep, bat,
  the app itself). Reproducible with `capture.sh`; also renders the 1280×640 social preview.
- **Landing page** in `site/` (static HTML, no build step) deployed by `pages.yml` with
  `actions/deploy-pages`; assets copied in from `assets/` so there is one source for images.
  SEO: title/meta description with search phrases, canonical, Open Graph/Twitter card,
  `SoftwareApplication` + `FAQPage` JSON-LD, robots.txt, sitemap.xml, light/dark, mobile layout.
- **Positioning:** "Obtainium for the desktop" — an existing, searched-for name that explains the
  product in three words.
- **Metadata:** authors/repository/homepage/keywords/categories in the workspace `Cargo.toml`
  (inherited by both crates), description/keywords/repository in `package.json`,
  publisher/homepage/copyright/license in `tauri.conf.json`.
- **Community files:** LICENSE (MIT, "Philippe S" from git config), SECURITY.md (private
  advisories, scope, how installs are protected), CONTRIBUTING.md, CODE_OF_CONDUCT.md
  (links to Contributor Covenant 2.1 rather than copying it), issue forms (bug, feature,
  asset-not-detected), PR template, Dependabot (cargo, npm, actions; monthly, grouped).
- **Launch kit** (`docs/marketing/launch-kit.md`): go-public checklist, description + topics
  with a `gh repo edit` command, SEO notes, Show HN/Reddit/short-post drafts, article outline,
  awesome lists and directories, distribution follow-ups.

## Rejected alternatives
- Keeping self-hosted runners with the fork guard — guard is bypassable (see above).
- Requiring approval for all outside-collaborator workflows as the only defence — easy to
  misconfigure, and hosted runners cost nothing for public repos.
- Putting the site in `docs/` (Pages "deploy from branch") — `docs/` already holds task records.
- A static-site generator for the landing page — one page, no build step needed.
- Running `gh repo edit` / uploading the social preview / posting — outward-facing; left to the
  maintainer with exact commands.
- Adding a CHANGELOG — GitHub generated release notes serve that role.

## What changed
- `.github/workflows/ci.yml`, `release.yml` (hosted runners, release body), new `pages.yml`
- `.github/ISSUE_TEMPLATE/*`, `PULL_REQUEST_TEMPLATE.md`, `dependabot.yml`
- `LICENSE`, `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `README.md`
- `docs/development.md`, `docs/marketing/launch-kit.md`
- `site/index.html`, `site/robots.txt`, `site/sitemap.xml`
- `scripts/screenshots/{demo.html,demo.ts,social.html,capture.sh}`
- `assets/screenshots/{updates,apps,settings}.png`, `assets/social-preview.png`
- `Cargo.toml`, `crates/updater-core/Cargo.toml`, `src-tauri/Cargo.toml`, `package.json`,
  `src-tauri/tauri.conf.json` (metadata only)

## Evidence
- `cargo metadata` parses; `npx tauri info` reads the config; `npm run check`: 0 errors.
- Screenshots and landing page rendered in headless Chrome at 1280 px and 390 px and inspected.
- Secret scan of the tree (`ghp_`, `github_pat_`, B2 key IDs, private keys, `/home/phil`):
  no hits apart from the token input placeholder. No git history yet, so nothing to rewrite.

## Corrections
- First screenshot showed "Last check: 10/11/58772" — `last_check` is Unix seconds, not ms.
  Fixed in the demo data.
- First landing render stretched images (HTML `width`/`height` attributes without
  `height: auto`) and hid `<code>` in the hero; fixed.
- Removed an unverified "checksum verification" comparison row about Obtainium from the site.

## Left undone
- The new workflows have not run on GitHub yet (first push will tell). `ci.yml` was not tested
  with `act`.
- `tauri.conf.json` `publisher` changes the MSI Manufacturer from the default ("phil", from the
  identifier) to "Philippe S". Harmless before the first public release; do not change it after.
- GitHub settings, Pages enablement, social preview upload, topics, posts: see the launch kit.
- Code signing, winget/Scoop manifests, Fedora/Arch/AppImage support.
- Screenshots show Linux only; Windows screenshots would need the app running on Windows.

## How to verify
```bash
npm run check
npm run dev &  scripts/screenshots/capture.sh
```
Open `site/index.html` after copying assets as `pages.yml` does, or run the `pages` workflow.
After pushing, check *Actions* for green `ci` on both OSes.
