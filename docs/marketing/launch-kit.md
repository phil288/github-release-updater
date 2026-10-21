# Launch kit

Everything needed to publish the repository and announce it. Copy, adapt, post.
Nothing here has been posted or applied yet.

## 1. Go-public checklist (in order)

- [ ] Merge `feat/initial-app` into `main`.
- [ ] Remove the self-hosted runners from the repository, if any were registered
      (*Settings → Actions → Runners*). The workflows now use GitHub-hosted runners only.
      A self-hosted runner attached to a public repository can be used by any fork pull request.
- [ ] Make the repository public (*Settings → General → Danger Zone → Change visibility*).
- [ ] Set description, website and topics (section 2).
- [ ] Upload `assets/social-preview.png` (*Settings → General → Social preview*).
- [ ] Enable GitHub Pages: *Settings → Pages → Source: GitHub Actions*, then run the `pages` workflow.
- [ ] Enable *Discussions* (the issue template links to it) and *Private vulnerability reporting*
      (*Settings → Code security*; SECURITY.md links to it).
- [ ] Enable Dependabot alerts and security updates (*Settings → Code security*).
- [ ] Protect `main`: require pull requests and the `ci` checks (`linux`, `windows`).
- [ ] *Settings → Actions → General → Fork pull request workflows*: require approval for
      first-time contributors (default; keep it).
- [ ] Tag `v0.1.0` and push; check the release has the `.deb`, `.msi` and `SHA256SUMS.txt`.
- [ ] Install the published `.deb` and `.msi` on clean machines once (the Windows path was
      never run end to end, see `docs/tasks/`).
- [ ] Pin the repository on your GitHub profile.
- [ ] Submit the site to Google Search Console and Bing Webmaster Tools (sitemap:
      `https://phil288.github.io/github-release-updater/sitemap.xml`).

## 2. Repository metadata

**Description** (≤ 350 chars, shown in search results):

> Auto-update apps installed from GitHub releases. Tray app for Ubuntu and Windows that installs new .deb and .msi release packages after your approval, with SHA-256 verification. Like Obtainium, for the desktop.

**Website:** `https://phil288.github.io/github-release-updater/`

**Topics** (max 20; these are what GitHub topic pages and search match on):

`github-releases`, `auto-update`, `updater`, `update-manager`, `package-manager`, `deb`,
`msi`, `ubuntu`, `debian`, `windows`, `linux`, `desktop-app`, `tray-app`, `tauri`, `rust`,
`svelte`, `obtainium`, `github-release`, `release-monitoring`, `software-updater`

Apply with the GitHub CLI (run it yourself; it changes the public repository):

```bash
gh repo edit phil288/github-release-updater \
  --description "Auto-update apps installed from GitHub releases. Tray app for Ubuntu and Windows that installs new .deb and .msi release packages after your approval, with SHA-256 verification. Like Obtainium, for the desktop." \
  --homepage "https://phil288.github.io/github-release-updater/" \
  --enable-discussions \
  --add-topic github-releases,auto-update,updater,update-manager,package-manager,deb,msi,ubuntu,debian,windows,linux,desktop-app,tray-app,tauri,rust,svelte,obtainium,github-release,release-monitoring,software-updater
```

## 3. SEO notes

Search intent this project answers (use these phrases in posts, titles and answers):

- "how to auto update deb installed from github"
- "update apps installed from github releases ubuntu"
- "obtainium for linux / for windows / for desktop"
- "github release notifier desktop"
- "auto update msi from github"

What is in place:

- README: keyword-led tagline in the first 160 characters, descriptive alt text, FAQ phrased as
  questions people search for.
- Landing page (`site/index.html`): title and meta description with the key phrases, canonical URL,
  Open Graph / Twitter card with `social-preview.png`, `SoftwareApplication` and `FAQPage`
  JSON-LD, `robots.txt`, `sitemap.xml`.
- Package metadata (`Cargo.toml`, `package.json`, `tauri.conf.json`): description, homepage,
  repository, keywords.

Backlinks matter most for a new repository. Section 5 lists where to get them.

## 4. Launch posts

### Show HN

**Title:** Show HN: GitHub Release Updater – auto-update .deb/.msi apps installed from GitHub

> Many desktop apps I use ship only as a .deb or .msi on their GitHub releases page: no apt repo,
> no store, no built-in updater. I kept falling behind, so I built a small tray app (Rust + Tauri +
> Svelte) for Ubuntu and Windows that watches those repositories and installs new releases after I
> approve.
>
> It picks the asset for your OS/CPU, verifies SHA-256 (GitHub's asset digest, .sha256 files or
> SHA256SUMS), and installs with the OS tools: pkexec + apt on Ubuntu (so dependencies resolve),
> elevated msiexec on Windows. Nothing installs in the background.
>
> It's MIT licensed, no telemetry. I'd love feedback on asset selection: if you have a repo where
> it picks the wrong file, I want to know.
>
> https://github.com/phil288/github-release-updater

Post Tuesday–Thursday, 14:00–16:00 UTC. Stay around for 2–3 hours to answer comments.

### Reddit

Subreddits: r/Ubuntu, r/linux, r/linuxapps, r/opensource, r/selfhosted (frame as homelab
desktops), r/Windows11, r/rust (focus on the Tauri/Rust build), r/sveltejs (UI side). Read
each subreddit's self-promotion rules first; post to one or two per day, not all at once.

**Title:** I made a tray app that keeps apps installed from GitHub releases up to date (.deb + .msi)

> If you install apps like draw.io or LocalSend from their GitHub releases page, you know they never
> update themselves. GitHub Release Updater watches those repos, notifies you when there's a new
> release, and installs the .deb (via apt, so dependencies resolve) or .msi when you click Install.
>
> - Picks the right asset for your CPU, regex override per app
> - SHA-256 verification, GitHub-hosted downloads only
> - polkit / UAC prompt every time, nothing installs silently
> - Backup settings to JSON or Backblaze B2
>
> Free, MIT, Rust + Tauri. Feedback welcome, especially repos where it picks the wrong file.
> [screenshot] https://github.com/phil288/github-release-updater

### Short post (Mastodon / Bluesky / X / LinkedIn)

> Apps installed from GitHub releases never update themselves. So I built GitHub Release Updater:
> a tray app for Ubuntu & Windows that installs new .deb/.msi releases after you approve, with
> SHA-256 checks. Like Obtainium, for the desktop. Open source, Rust + Tauri.
> https://github.com/phil288/github-release-updater #linux #ubuntu #opensource #rustlang #tauri

Attach `assets/social-preview.png` or `assets/screenshots/updates.png`.

### Blog / dev.to article outline

**Title:** "Keeping GitHub-release apps up to date on Ubuntu and Windows"

1. The problem: the long tail of apps distributed only through GitHub releases.
2. Picking the right asset: architecture names, `-cli`/`-portable` variants, scoring.
3. Verifying downloads: GitHub's new asset `digest`, `.sha256`, `SHA256SUMS`.
4. Installing safely: why `pkexec apt-get install ./file.deb` and not `dpkg -i`; elevating
   `msiexec` with `ShellExecuteExW runas`; exit codes for "cancelled".
5. Tauri 2 notes: tray click events on Linux (KSNI vs AppIndicator), single instance, autostart.
6. Try it / contribute.

Cross-post to dev.to, Hashnode and your own blog with a canonical link.

## 5. Directories and lists (backlinks)

Submit after v0.1.0 is published and the README has screenshots (it does):

- [awesome-linux-software](https://github.com/luong-komorebi/Awesome-Linux-Software) – System / Package managers
- [awesome-tauri](https://github.com/tauri-apps/awesome-tauri) – Applications
- [awesome-rust](https://github.com/rust-unofficial/awesome-rust) – Applications / System tools
- [awesome-windows](https://github.com/0PandaDEV/awesome-windows) – Utilities
- [AlternativeTo](https://alternativeto.net) – list it as an alternative to Obtainium, Topgrade, WingetUI/UniGetUI
- [Linux Uprising](https://www.linuxuprising.com), [OMG! Ubuntu](https://www.omgubuntu.co.uk/contact),
  [It's FOSS](https://itsfoss.com/contact-us/) – tip forms for app coverage
- Product Hunt (developer tools), once there is a Windows release tested end to end

Check each list's contribution rules (most require a minimum star count or project age).

## 6. Distribution that also markets

Each one is a new discovery channel; tracked as follow-ups, not done:

- **winget** manifest (`winget install phil288.GitHubReleaseUpdater`) – needs a stable MSI URL
  per release (in place) and ideally code signing.
- **Scoop** bucket entry.
- **Flathub / AUR / .rpm** – would need the app to support those formats first.
- **Code signing** for the MSI (e.g. SignPath Foundation offers free signing for OSS) to remove
  the SmartScreen warning, the biggest friction for Windows users.

## 7. After launch

- Answer every issue in the first weeks; label good first issues (asset rules for specific repos
  are ideal).
- Use GitHub release notes as the changelog; announce notable releases on the same channels.
- Watch *Insights → Traffic* for referrers to see which channel worked.
