// Mirrors the serde types in crates/updater-core and src-tauri.

export interface Asset {
  name: string;
  browser_download_url: string;
  size: number;
  digest: string | null;
}

export interface TrackedApp {
  id: string;
  repo: string;
  name: string | null;
  enabled: boolean;
  include_prerelease: boolean;
  asset_pattern: { linux: string | null; windows: string | null };
  package_name: string | null;
  windows_display_name: string | null;
  installed_version: string | null;
  skipped_version: string | null;
  /** Included in automatic installs when `Settings.auto_update` is on. */
  auto_update: boolean;
}

export type MsiUi = "passive" | "full";

export type AppsView = "list" | "cards";

export interface Settings {
  schema_version: number;
  check_interval_hours: number;
  autostart: boolean;
  open_window_on_update: boolean;
  auto_update: boolean;
  msi_ui: MsiUi;
  require_checksum: boolean;
  apps_view: AppsView;
  apps: TrackedApp[];
}

export interface B2Config {
  key_id: string;
  bucket: string;
  /** Folder inside the bucket, normalised to `a/b/`; empty for the root. */
  prefix: string;
  auto_backup: boolean;
}

export interface B2State {
  config: B2Config;
  has_key: boolean;
}

export interface B2TestReport {
  bucket: string;
  location: string;
  backups_found: number;
  missing_capabilities: string[];
  prefix_problem: string | null;
}

export interface RemoteBackup {
  file_id: string;
  file_name: string;
  size: number;
  /** Milliseconds since the Unix epoch. */
  uploaded_at: number;
}

export type ImportMode = "replace" | "merge";

export interface UpdateInfo {
  app_id: string;
  app_name: string;
  repo: string;
  installed_version: string | null;
  tag: string;
  release_name: string | null;
  html_url: string;
  published_at: string | null;
  notes: string | null;
  prerelease: boolean;
  asset: Asset;
  release_assets: Asset[];
  /** Releases jumped over, newest first. */
  intermediate_tags: string[];
}

export type CheckResult =
  | { status: "up_to_date"; tag: string }
  | { status: "skipped"; tag: string }
  | ({ status: "available" } & UpdateInfo)
  | { status: "no_release" }
  | { status: "no_matching_asset"; tag: string; assets: string[] }
  | { status: "error"; message: string };

export interface Status {
  pending: UpdateInfo[];
  results: Record<string, CheckResult>;
  last_check: number | null;
  checking: boolean;
  installing: string | null;
}

export type Progress =
  | { stage: "downloading"; downloaded: number; total: number | null }
  | { stage: "verifying" }
  | { stage: "installing" }
  | { stage: "log"; line: string };

export type ProgressEvent = Progress & { app_id: string };

/** Sent when any install ends, including automatic ones. */
export interface FinishedEvent {
  app_id: string;
  stage: "done" | "cancelled" | "failed";
  message: string;
}

export type Outcome =
  | { kind: "installed" }
  | { kind: "installed_reboot_required" }
  | { kind: "uninstalled" }
  | { kind: "uninstalled_reboot_required" }
  | { kind: "cancelled" }
  | { kind: "failed"; code: number | null; message: string };

export interface InstallReport {
  outcome: Outcome;
  tag: string;
  file: string;
  verified: boolean;
  package_name: string | null;
}

export interface Preview {
  tag: string;
  selected: string | null;
  installer_assets: string[];
  checksum: string | null;
}

export interface Platform {
  os: "linux" | "windows";
  arch: "x64" | "arm64" | "x86" | "other";
}

export type Page = "updates" | "apps" | "settings";
