import { invoke } from "@tauri-apps/api/core";
import type {
  B2Config,
  B2State,
  B2TestReport,
  ImportMode,
  InstallReport,
  Outcome,
  Platform,
  Preview,
  RemoteBackup,
  Settings,
  Status,
  TrackedApp,
} from "./types";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  newApp: (repo: string) => invoke<TrackedApp>("new_app", { repo }),
  previewApp: (app: TrackedApp) => invoke<Preview | null>("preview_app", { app }),
  detectInstalled: (app: TrackedApp) => invoke<string | null>("detect_installed", { app }),
  getStatus: () => invoke<Status>("get_status"),
  checkNow: () => invoke<Status>("check_now"),
  installUpdate: (appId: string) => invoke<InstallReport>("install_update", { appId }),
  skipVersion: (appId: string) => invoke<void>("skip_version", { appId }),
  uninstallApp: (appId: string) => invoke<Outcome>("uninstall_app", { appId }),
  exportBackup: () => invoke<string | null>("export_backup"),
  importBackup: (mode: ImportMode) => invoke<Settings | null>("import_backup", { mode }),
  getB2Config: () => invoke<B2State>("get_b2_config"),
  saveB2Config: (config: B2Config, appKey: string | null) => invoke<B2State>("save_b2_config", { config, appKey }),
  clearB2Config: () => invoke<B2State>("clear_b2_config"),
  testB2: (config: B2Config, appKey: string | null) => invoke<B2TestReport>("test_b2", { config, appKey }),
  b2BackupNow: () => invoke<string>("b2_backup_now"),
  b2ListBackups: () => invoke<RemoteBackup[]>("b2_list_backups"),
  b2Restore: (fileId: string, mode: ImportMode) => invoke<Settings>("b2_restore", { fileId, mode }),
  hasToken: () => invoke<boolean>("has_token"),
  setToken: (token: string | null) => invoke<void>("set_token", { token }),
  openReleasePage: (url: string) => invoke<void>("open_release_page", { url }),
  platform: () => invoke<Platform>("platform"),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}
