import type { Page, Platform, Settings, Status } from "./types";

export interface InstallProgress {
  stage: "downloading" | "verifying" | "installing" | "done" | "cancelled" | "failed";
  downloaded: number;
  total: number | null;
  log: string[];
  message: string | null;
}

export const store = $state({
  page: "updates" as Page,
  settings: null as Settings | null,
  status: null as Status | null,
  platform: null as Platform | null,
  progress: {} as Record<string, InstallProgress>,
});

export function emptyProgress(): InstallProgress {
  return { stage: "downloading", downloaded: 0, total: null, log: [], message: null };
}
