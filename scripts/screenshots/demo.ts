// Screenshot harness: runs the real UI in a plain browser with mocked Tauri IPC and demo data.
// Start `npm run dev`, then open http://localhost:1420/scripts/screenshots/demo.html?page=updates
// (page = updates | apps | settings). Used by scripts/screenshots/capture.sh.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Settings, Status, TrackedApp, UpdateInfo } from "../../src/lib/types";

const app = (id: string, repo: string, name: string, installed: string, pkg: string): TrackedApp => ({
  id,
  repo,
  name,
  enabled: true,
  include_prerelease: false,
  asset_pattern: { linux: null, windows: null },
  package_name: pkg,
  windows_display_name: null,
  installed_version: installed,
  skipped_version: null,
});

const apps = [
  app("1", "jgraph/drawio-desktop", "draw.io", "28.1.2", "drawio"),
  app("2", "localsend/localsend", "LocalSend", "1.16.1", "localsend"),
  app("3", "BurntSushi/ripgrep", "ripgrep", "14.1.1", "ripgrep"),
  app("4", "sharkdp/bat", "bat", "0.25.0", "bat"),
  app("5", "phil288/github-release-updater", "GitHub Release Updater", "0.1.0", "gh-release-updater"),
];

const settings: Settings = {
  schema_version: 1,
  check_interval_hours: 6,
  autostart: true,
  open_window_on_update: true,
  msi_ui: "passive",
  require_checksum: false,
  apps_view: "cards",
  apps,
};

const update = (a: TrackedApp, tag: string, asset: string, size: number, skipped: string[] = []): UpdateInfo => ({
  app_id: a.id,
  app_name: a.name!,
  repo: a.repo,
  installed_version: a.installed_version,
  tag,
  release_name: tag,
  html_url: `https://github.com/${a.repo}/releases/tag/${tag}`,
  published_at: "2026-10-18T09:12:00Z",
  notes: "Bug fixes and performance improvements.",
  prerelease: false,
  asset: { name: asset, browser_download_url: "", size, digest: "sha256:0" },
  release_assets: [],
  intermediate_tags: skipped,
});

const pending = [
  update(apps[0], "v28.2.5", "drawio-amd64-28.2.5.deb", 104_857_600, ["v28.2.0", "v28.1.5"]),
  update(apps[1], "v1.17.0", "LocalSend-1.17.0-linux-x86-64.deb", 18_350_080),
];

const status: Status = {
  pending,
  results: {
    "1": { status: "available", ...pending[0] },
    "2": { status: "available", ...pending[1] },
    "3": { status: "up_to_date", tag: "14.1.1" },
    "4": { status: "up_to_date", tag: "v0.25.0" },
    "5": { status: "up_to_date", tag: "v0.1.0" },
  },
  last_check: Math.floor(Date.now() / 1000) - 12 * 60,
  checking: false,
  installing: null,
};

mockIPC(
  (cmd) => {
    switch (cmd) {
      case "get_settings":
      case "save_settings":
        return settings;
      case "get_status":
      case "check_now":
        return status;
      case "platform":
        return { os: "linux", arch: "x64" };
      case "has_token":
        return true;
      case "get_b2_config":
        return { config: { key_id: "", bucket: "", prefix: "", auto_backup: false }, has_key: false };
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

const { store } = await import("../../src/lib/store.svelte");
const page = new URLSearchParams(location.search).get("page");
if (page === "apps" || page === "settings" || page === "updates") store.page = page;
await import("../../src/main");
