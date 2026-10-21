<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorText, formatBytes } from "../lib/api";
  import type { B2Config, B2TestReport, ImportMode, RemoteBackup, Settings } from "../lib/types";

  let { onRestored }: { onRestored: (settings: Settings) => void } = $props();

  const empty: B2Config = { key_id: "", bucket: "", prefix: "", auto_backup: false };
  let saved = $state<B2Config>({ ...empty });
  let draft = $state<B2Config>({ ...empty });
  let hasKey = $state(false);
  let appKey = $state("");
  let busy = $state<string | null>(null);
  let notice = $state<{ kind: "success" | "error"; text: string } | null>(null);
  let test = $state<B2TestReport | null>(null);
  let backups = $state<RemoteBackup[] | null>(null);

  const configured = $derived(!!saved.key_id && !!saved.bucket && hasKey);
  const dirty = $derived(
    !!appKey.trim() ||
      draft.key_id.trim() !== saved.key_id ||
      draft.bucket.trim() !== saved.bucket ||
      draft.prefix.trim().replace(/^\/+|\/+$/g, "") !== saved.prefix.replace(/\/$/, "") ||
      draft.auto_backup !== saved.auto_backup,
  );
  const canTest = $derived(!!draft.key_id.trim() && !!draft.bucket.trim() && (hasKey || !!appKey.trim()));

  onMount(async () => {
    const s = await api.getB2Config();
    apply(s.config, s.has_key);
  });

  function apply(config: B2Config, key: boolean) {
    saved = config;
    draft = { ...config };
    hasKey = key;
    appKey = "";
  }

  async function run(label: string, action: () => Promise<string | null>) {
    notice = null;
    busy = label;
    try {
      const text = await action();
      if (text) notice = { kind: "success", text };
    } catch (e) {
      notice = { kind: "error", text: errorText(e) };
    } finally {
      busy = null;
    }
  }

  const runTest = () =>
    run("test", async () => {
      test = null;
      test = await api.testB2($state.snapshot(draft), appKey.trim() || null);
      return null;
    });

  const save = () =>
    run("save", async () => {
      const s = await api.saveB2Config($state.snapshot(draft), appKey.trim() || null);
      apply(s.config, s.has_key);
      return "Backblaze settings saved.";
    });

  const clear = () =>
    run("clear", async () => {
      if (!confirm("Forget the Backblaze settings and delete the stored application key? Backups in the bucket are kept.")) return null;
      const s = await api.clearB2Config();
      apply(s.config, s.has_key);
      test = null;
      backups = null;
      return "Backblaze settings removed.";
    });

  const backupNow = () =>
    run("backup", async () => {
      const name = await api.b2BackupNow();
      if (backups) backups = await api.b2ListBackups();
      return `Backup uploaded as ${name}`;
    });

  const list = () =>
    run("list", async () => {
      backups = await api.b2ListBackups();
      return null;
    });

  const restore = (b: RemoteBackup, mode: ImportMode) =>
    run(`restore-${b.file_id}`, async () => {
      const when = formatDate(b.uploaded_at);
      const question =
        mode === "replace"
          ? `Replace all current settings and applications with the backup from ${when}?`
          : `Merge the backup from ${when} into the current settings?`;
      if (!confirm(question)) return null;
      const s = await api.b2Restore(b.file_id, mode);
      onRestored(s);
      return `Restored ${s.apps.length} application(s) from the backup of ${when} (${mode}).`;
    });

  function formatDate(ms: number): string {
    return new Date(ms).toLocaleString();
  }

  function shortName(name: string): string {
    return name.slice(name.lastIndexOf("/") + 1);
  }
</script>

<div class="card">
  <h3 style="margin-bottom: 4px">Backblaze B2 backup</h3>
  <p class="muted small">
    Uploads backups to a B2 bucket, one file per backup. Create an application key restricted to the bucket with
    <code>writeFiles</code>, <code>listFiles</code> and <code>readFiles</code>. The key is stored only on this machine and
    never included in backups.
  </p>
  {#if notice}<div class="notice {notice.kind}">{notice.text}</div>{/if}

  <div class="grid2">
    <label class="field"><span>Key ID</span>
      <input type="text" bind:value={draft.key_id} placeholder="0051a2b3c4d5e6f0000000001" autocomplete="off" /></label>
    <label class="field"><span>Application key</span>
      <input type="password" bind:value={appKey} placeholder={hasKey ? "•••••••• (set)" : "K005…"} autocomplete="off" /></label>
    <label class="field"><span>Bucket</span>
      <input type="text" bind:value={draft.bucket} placeholder="my-backups" autocomplete="off" /></label>
    <label class="field"><span>Folder (optional)</span>
      <input type="text" bind:value={draft.prefix} placeholder="laptop" autocomplete="off" /></label>
  </div>
  <label class="check" style="margin-top: 12px">
    <input type="checkbox" bind:checked={draft.auto_backup} /> Back up automatically when settings change
  </label>

  <div class="row" style="margin-top: 12px">
    <button onclick={runTest} disabled={!canTest || !!busy}>{busy === "test" ? "Testing…" : "Test connection"}</button>
    <button class="primary" onclick={save} disabled={!dirty || !!busy}>Save</button>
    <span class="spacer"></span>
    {#if saved.key_id || hasKey}<button class="danger" onclick={clear} disabled={!!busy}>Remove</button>{/if}
  </div>

  {#if test}
    {@const problems = test.missing_capabilities.length > 0 || !!test.prefix_problem}
    <div class="notice {problems ? 'warn-box' : 'success'}" style="margin: 12px 0 0">
      <div>Connected to bucket <strong>{test.bucket}</strong>. Backups go to <code>{test.location}</code>.</div>
      <div>{test.backups_found} existing backup(s) found.</div>
      {#if test.missing_capabilities.length}
        <div>The key lacks: {test.missing_capabilities.join(", ")}.</div>
      {/if}
      {#if test.prefix_problem}<div>Problem: {test.prefix_problem}.</div>{/if}
    </div>
  {/if}

  <div class="row" style="margin-top: 12px; border-top: 1px solid var(--border); padding-top: 12px">
    <button onclick={backupNow} disabled={!configured || dirty || !!busy}>{busy === "backup" ? "Uploading…" : "Back up now"}</button>
    <button onclick={list} disabled={!configured || dirty || !!busy}>{busy === "list" ? "Loading…" : backups ? "Refresh list" : "View backups"}</button>
    {#if dirty && configured}<span class="muted small">Save the changes first.</span>{/if}
    {#if !configured}<span class="muted small">Save a key ID, application key and bucket to use these.</span>{/if}
  </div>

  {#if backups}
    {#if backups.length === 0}
      <p class="muted small" style="margin-bottom: 0">No backups in this folder yet.</p>
    {:else}
      <table class="backups">
        <thead><tr><th>Date</th><th>File</th><th>Size</th><th></th></tr></thead>
        <tbody>
          {#each backups as b (b.file_id)}
            <tr>
              <td>{formatDate(b.uploaded_at)}</td>
              <td><code title={b.file_name}>{shortName(b.file_name)}</code></td>
              <td class="muted">{formatBytes(b.size)}</td>
              <td class="actions">
                <button onclick={() => restore(b, "merge")} disabled={!!busy}>Merge</button>
                <button onclick={() => restore(b, "replace")} disabled={!!busy}>Replace</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  {/if}
</div>

<style>
  .warn-box { border-color: var(--warn); color: var(--warn); }
  .backups { width: 100%; border-collapse: collapse; margin-top: 12px; font-size: 0.92em; }
  .backups th { text-align: left; color: var(--muted); font-weight: normal; padding: 4px 6px; }
  .backups td { padding: 4px 6px; border-top: 1px solid var(--border); vertical-align: middle; }
  .backups code { word-break: break-all; }
  .actions { white-space: nowrap; text-align: right; }
  .actions button { padding: 3px 8px; }
</style>
