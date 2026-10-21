<script lang="ts">
  import { api, errorText, formatBytes } from "../lib/api";
  import { emptyProgress, store, type InstallProgress } from "../lib/store.svelte";
  import type { CheckResult, UpdateInfo } from "../lib/types";

  let error = $state<string | null>(null);
  let checking = $state(false);
  let updatingAll = $state(false);

  const status = $derived(store.status);
  const busy = $derived(checking || !!status?.checking);
  const apps = $derived(store.settings?.apps.filter((a) => a.enabled) ?? []);
  const installer = $derived(store.platform?.os === "windows" ? "UAC" : "password");

  async function checkNow() {
    error = null;
    checking = true;
    try {
      store.status = await api.checkNow();
      store.settings = await api.getSettings();
    } catch (e) {
      error = errorText(e);
    } finally {
      checking = false;
    }
  }

  async function install(u: UpdateInfo): Promise<InstallProgress["stage"]> {
    error = null;
    const p = emptyProgress();
    p.log.push(`Downloading ${u.asset.name}…`);
    store.progress[u.app_id] = p;
    try {
      const report = await api.installUpdate(u.app_id);
      const live = store.progress[u.app_id];
      switch (report.outcome.kind) {
        case "installed":
        case "installed_reboot_required":
          live.stage = "done";
          live.message =
            `${u.app_name} ${report.tag} installed` +
            (report.outcome.kind === "installed_reboot_required" ? " — restart required." : ".");
          break;
        case "cancelled":
          live.stage = "cancelled";
          live.message = "Installation cancelled.";
          break;
        case "failed":
          live.stage = "failed";
          live.message = report.outcome.message;
          break;
      }
      store.settings = await api.getSettings();
    } catch (e) {
      const live = store.progress[u.app_id];
      live.stage = "failed";
      live.message = errorText(e);
    }
    return store.progress[u.app_id].stage;
  }

  // Installs every pending update one after another (the backend allows only one
  // install at a time). A failure moves on to the next app; cancelling an
  // elevation prompt stops the batch, since the user declined.
  async function updateAll() {
    updatingAll = true;
    try {
      for (const u of [...(status?.pending ?? [])]) {
        if (!store.status?.pending.some((p) => p.app_id === u.app_id)) continue;
        if ((await install(u)) === "cancelled") break;
      }
    } finally {
      updatingAll = false;
    }
  }

  async function skip(u: UpdateInfo) {
    try {
      await api.skipVersion(u.app_id);
      store.settings = await api.getSettings();
    } catch (e) {
      error = errorText(e);
    }
  }

  function describe(r: CheckResult | undefined): { text: string; cls: string } {
    if (!r) return { text: "Not checked yet", cls: "muted" };
    switch (r.status) {
      case "up_to_date": return { text: `Up to date (${r.tag})`, cls: "ok" };
      case "skipped": return { text: `${r.tag} skipped`, cls: "muted" };
      case "available": return { text: `${r.tag} available`, cls: "warn" };
      case "no_release": return { text: "No published release", cls: "muted" };
      case "no_matching_asset":
        return { text: `${r.tag}: no matching ${store.platform?.os === "windows" ? ".msi" : ".deb"} asset — set a pattern`, cls: "warn" };
      case "error": return { text: r.message, cls: "danger-text" };
    }
  }

  function when(ts: number | null | undefined) {
    return ts ? new Date(ts * 1000).toLocaleString() : "never";
  }

  // Keep finished progress visible for apps no longer pending.
  const finished = $derived(
    Object.entries(store.progress).filter(
      ([id, p]) => ["done", "failed", "cancelled"].includes(p.stage) && !status?.pending.some((u) => u.app_id === id),
    ),
  );

  function dismissAll() {
    for (const [id] of finished) delete store.progress[id];
  }
</script>

<div class="row" style="margin-bottom: 12px">
  <h2 style="margin: 0">Updates</h2>
  <span class="spacer"></span>
  <span class="muted small">Last check: {busy ? "checking…" : when(status?.last_check)}</span>
  <button onclick={checkNow} disabled={busy || updatingAll}>{busy ? "Checking…" : "Check now"}</button>
  {#if (status?.pending.length ?? 0) > 1}
    <button class="primary" onclick={updateAll} disabled={busy || updatingAll || !!status?.installing}>
      {updatingAll ? "Updating…" : `Update all (${status?.pending.length})`}
    </button>
  {/if}
</div>

{#if error}<div class="notice error">{error}</div>{/if}

{#if apps.length === 0}
  <div class="card">
    <p>No applications tracked yet.</p>
    <button class="primary" onclick={() => (store.page = "apps")}>Add an application</button>
  </div>
{/if}

{#each status?.pending ?? [] as u (u.app_id)}
  {@const p = store.progress[u.app_id]}
  {@const running = status?.installing === u.app_id}
  <div class="card">
    <div class="row">
      <h3>{u.app_name}</h3>
      <span class="muted">{u.installed_version ?? "not installed"} → <strong>{u.tag}</strong></span>
      {#if u.prerelease}<span class="badge warn">pre-release</span>{/if}
      <span class="spacer"></span>
      <button class="link" onclick={() => api.openReleasePage(u.html_url)}>Release page</button>
    </div>
    <p class="muted small">
      {u.asset.name} · {formatBytes(u.asset.size)}{u.published_at ? ` · published ${new Date(u.published_at).toLocaleDateString()}` : ""}
    </p>
    {#if u.intermediate_tags.length}
      <p class="small muted">
        Upgrades directly to the latest version, skipping {u.intermediate_tags.length}
        intermediate release{u.intermediate_tags.length > 1 ? "s" : ""}: {u.intermediate_tags.join(", ")}.
      </p>
    {/if}
    {#if u.notes}
      <details><summary class="small">Release notes</summary><pre class="log">{u.notes}</pre></details>
    {/if}

    {#if p && (running || p.stage !== "downloading" || p.downloaded > 0)}
      <div style="margin-top: 10px">
        {#if p.stage === "downloading"}
          <div class="small">Downloading {formatBytes(p.downloaded)}{p.total ? ` of ${formatBytes(p.total)}` : ""}</div>
          <progress value={p.downloaded} max={p.total ?? undefined}></progress>
        {:else if p.stage === "verifying"}
          <div class="small">Verifying checksum…</div>
          <progress></progress>
        {:else if p.stage === "installing"}
          <div class="small">Installing — approve the {installer} prompt…</div>
          <progress></progress>
        {:else if p.message}
          <div class="small" class:ok={p.stage === "done"} class:danger-text={p.stage === "failed"}>{p.message}</div>
        {/if}
        {#if p.log.length}<pre class="log">{p.log.join("\n")}</pre>{/if}
      </div>
    {/if}

    <div class="row" style="margin-top: 10px">
      <button class="primary" onclick={() => install(u)} disabled={updatingAll || !!status?.installing}>
        {running ? "Installing…" : "Install"}
      </button>
      <button onclick={() => skip(u)} disabled={running || updatingAll}>Skip this version</button>
    </div>
  </div>
{/each}

{#if finished.length > 1}
  <div class="row" style="margin-bottom: 8px">
    <span class="muted small">{finished.length} completed</span>
    <span class="spacer"></span>
    <button class="link small" onclick={dismissAll}>Dismiss all</button>
  </div>
{/if}

{#each finished as [id, p] (id)}
  <div class="card">
    <div class="small" class:ok={p.stage === "done"} class:danger-text={p.stage === "failed"}>{p.message}</div>
    {#if p.log.length}<details><summary class="small">Installer output</summary><pre class="log">{p.log.join("\n")}</pre></details>{/if}
    <button class="link small" onclick={() => delete store.progress[id]}>Dismiss</button>
  </div>
{/each}

{#if apps.length}
  <div class="card">
    <h3 style="margin-bottom: 8px">Tracked applications</h3>
    <table>
      <tbody>
        {#each apps as a (a.id)}
          {@const d = describe(status?.results[a.id])}
          <tr>
            <td><strong>{a.name || a.repo.split("/")[1]}</strong> <span class="muted small">{a.repo}</span></td>
            <td class="muted">{a.installed_version ?? "—"}</td>
            <td class={d.cls}>{d.text}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  table { width: 100%; border-collapse: collapse; }
  td { padding: 6px 4px; border-top: 1px solid var(--border); vertical-align: top; }
  tr:first-child td { border-top: none; }
  summary { cursor: pointer; }
</style>
