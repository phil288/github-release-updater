<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorText } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import BackblazeBackup from "./BackblazeBackup.svelte";
  import type { Settings } from "../lib/types";

  let draft = $state<Settings>($state.snapshot(store.settings!));
  let tokenSet = $state(false);
  let token = $state("");
  let notice = $state<{ kind: "success" | "error"; text: string } | null>(null);

  onMount(async () => {
    tokenSet = await api.hasToken();
  });

  async function run(action: () => Promise<string | null>) {
    notice = null;
    try {
      const text = await action();
      if (text) notice = { kind: "success", text };
    } catch (e) {
      notice = { kind: "error", text: errorText(e) };
    }
  }

  const save = () =>
    run(async () => {
      // Apps are edited on their own page; keep the latest list.
      store.settings = await api.saveSettings({ ...draft, apps: store.settings!.apps, apps_view: store.settings!.apps_view });
      draft = $state.snapshot(store.settings);
      return "Settings saved.";
    });

  const saveToken = (value: string | null) =>
    run(async () => {
      await api.setToken(value);
      tokenSet = !!value;
      token = "";
      return value ? "Token saved." : "Token removed.";
    });

  const backup = () =>
    run(async () => {
      const path = await api.exportBackup();
      return path ? `Backup written to ${path}` : null;
    });

  function restored(s: Settings) {
    store.settings = s;
    draft = $state.snapshot(s);
  }

  const restore = (mode: "replace" | "merge") =>
    run(async () => {
      if (mode === "replace" && !confirm("Replace all current settings and applications with the backup?")) return null;
      const s = await api.importBackup(mode);
      if (!s) return null;
      restored(s);
      return `Restored ${s.apps.length} application(s) (${mode}).`;
    });
</script>

<h2>Settings</h2>
{#if notice}<div class="notice {notice.kind}">{notice.text}</div>{/if}

<div class="card">
  <h3 style="margin-bottom: 12px">General</h3>
  <div class="grid2">
    <label class="field"><span>Check every (hours)</span>
      <input type="number" min="1" max="168" bind:value={draft.check_interval_hours} /></label>
    {#if store.platform?.os === "windows"}
      <label class="field"><span>MSI installer window</span>
        <select bind:value={draft.msi_ui}>
          <option value="passive">Progress bar only</option>
          <option value="full">Full installer wizard</option>
        </select></label>
    {/if}
  </div>
  <div style="display: grid; gap: 8px; margin-top: 12px">
    <label class="check"><input type="checkbox" bind:checked={draft.autostart} /> Start at login (in the tray)</label>
    <label class="check"><input type="checkbox" bind:checked={draft.open_window_on_update} /> Open this window when updates are found</label>
    <label class="check"><input type="checkbox" bind:checked={draft.require_checksum} /> Only install releases that publish a SHA-256 checksum</label>
  </div>
  <div class="row" style="margin-top: 12px"><button class="primary" onclick={save}>Save</button></div>
</div>

<div class="card">
  <h3 style="margin-bottom: 4px">GitHub token (optional)</h3>
  <p class="muted small">
    Without a token GitHub allows 60 API requests per hour. A fine-grained token with no permissions raises this to 5,000.
    Stored only on this machine and never included in backups.
  </p>
  <div class="row">
    <input type="password" style="flex: 1; width: auto" bind:value={token} placeholder={tokenSet ? "•••••••• (set)" : "github_pat_…"} />
    <button onclick={() => saveToken(token)} disabled={!token.trim()}>Save token</button>
    {#if tokenSet}<button class="danger" onclick={() => saveToken(null)}>Remove</button>{/if}
  </div>
</div>

<div class="card">
  <h3 style="margin-bottom: 4px">Backup and restore</h3>
  <p class="muted small">
    Saves settings and tracked applications to a JSON file. Restoring on another machine re-detects installed versions.
  </p>
  <div class="row">
    <button onclick={backup}>Back up…</button>
    <button onclick={() => restore("merge")}>Restore and merge…</button>
    <button onclick={() => restore("replace")}>Restore and replace…</button>
  </div>
</div>

<BackblazeBackup onRestored={restored} />
