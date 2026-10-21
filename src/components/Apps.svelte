<script lang="ts">
  import { api, errorText } from "../lib/api";
  import { emptyProgress, store } from "../lib/store.svelte";
  import type { AppsView, TrackedApp } from "../lib/types";
  import AppEditor from "./AppEditor.svelte";

  let repoInput = $state("");
  let adding = $state(false);
  let error = $state<string | null>(null);
  let editing = $state<TrackedApp | null>(null);
  let isNew = $state(false);

  async function add(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    adding = true;
    try {
      const app = await api.newApp(repoInput);
      if (store.settings!.apps.some((a) => a.repo.toLowerCase() === app.repo.toLowerCase())) {
        error = `${app.repo} is already tracked.`;
        return;
      }
      editing = app;
      isNew = true;
      repoInput = "";
    } catch (err) {
      error = errorText(err);
    } finally {
      adding = false;
    }
  }

  async function persist(apps: TrackedApp[]) {
    error = null;
    try {
      store.settings = await api.saveSettings({ ...store.settings!, apps });
      return true;
    } catch (e) {
      error = errorText(e);
      return false;
    }
  }

  async function saveApp(app: TrackedApp) {
    const apps = store.settings!.apps.slice();
    const i = apps.findIndex((a) => a.id === app.id);
    if (i >= 0) apps[i] = app;
    else apps.push(app);
    if (await persist(apps)) editing = null;
  }

  async function remove(app: TrackedApp) {
    if (!confirm(`Stop tracking ${app.repo}? Installed software is not removed; use Uninstall for that.`)) return;
    await persist(store.settings!.apps.filter((a) => a.id !== app.id));
  }

  async function toggle(app: TrackedApp) {
    await persist(store.settings!.apps.map((a) => (a.id === app.id ? { ...a, enabled: !a.enabled } : a)));
  }

  // Result of the last uninstall per app: message plus the package manager output.
  let uninstallResult = $state<Record<string, { ok: boolean; message: string; log: string[] }>>({});
  const elevation = $derived(store.platform?.os === "windows" ? "UAC" : "password");

  function canUninstall(app: TrackedApp) {
    return !!app.installed_version && (store.platform?.os === "windows" || !!app.package_name);
  }

  async function uninstall(app: TrackedApp) {
    const name = app.name || app.repo.split("/")[1];
    if (!confirm(`Uninstall ${name} from this computer? You will be asked for your ${elevation}.`)) return;
    error = null;
    delete uninstallResult[app.id];
    // Package manager output arrives as install-progress log events (see App.svelte).
    store.progress[app.id] = emptyProgress();
    let ok = false;
    let message: string;
    try {
      const outcome = await api.uninstallApp(app.id);
      switch (outcome.kind) {
        case "uninstalled":
        case "uninstalled_reboot_required":
          ok = true;
          message = `${name} was uninstalled` + (outcome.kind === "uninstalled_reboot_required" ? " — restart required." : ".");
          break;
        case "cancelled":
          message = "Uninstall cancelled.";
          break;
        case "failed":
          message = outcome.message;
          break;
        default:
          message = `Unexpected result: ${outcome.kind}`;
      }
    } catch (e) {
      message = errorText(e);
    }
    uninstallResult[app.id] = { ok, message, log: store.progress[app.id]?.log ?? [] };
    delete store.progress[app.id];
    store.settings = await api.getSettings();
    if (ok && confirm(`${message}\n\nAlso stop tracking ${app.repo}? (It has been disabled.)`)) {
      await persist(store.settings!.apps.filter((a) => a.id !== app.id));
      delete uninstallResult[app.id];
    }
  }

  const view = $derived<AppsView>(store.settings?.apps_view ?? "list");

  async function setView(v: AppsView) {
    if (v === view || !store.settings) return;
    error = null;
    try {
      store.settings = await api.saveSettings({ ...store.settings, apps_view: v });
    } catch (e) {
      error = errorText(e);
    }
  }

  function edit(app: TrackedApp) {
    editing = $state.snapshot(app);
    isNew = false;
  }
</script>

<div class="row" style="margin-bottom: 12px">
  <h2 style="margin: 0">Applications</h2>
  <span class="spacer"></span>
  {#if !editing}
    <div class="segmented" role="group" aria-label="Layout">
      <button class:active={view === "list"} aria-pressed={view === "list"} onclick={() => setView("list")} title="List view">☰ List</button>
      <button class:active={view === "cards"} aria-pressed={view === "cards"} onclick={() => setView("cards")} title="Card view">▦ Cards</button>
    </div>
  {/if}
</div>
{#if error}<div class="notice error">{error}</div>{/if}

{#if editing}
  <AppEditor app={editing} {isNew} onsave={saveApp} oncancel={() => (editing = null)} />
{:else}
  <form class="card row" onsubmit={add}>
    <input
      type="text"
      style="flex: 1; width: auto"
      placeholder="owner/name or https://github.com/owner/name/releases/tag/v1.0"
      bind:value={repoInput}
    />
    <button class="primary" type="submit" disabled={adding || !repoInput.trim()}>{adding ? "Looking up…" : "Add"}</button>
  </form>

  <div class:app-grid={view === "cards"}>
  {#each store.settings?.apps ?? [] as app (app.id)}
    {@const running = store.status?.installing === app.id}
    {@const live = store.progress[app.id]}
    {@const done = uninstallResult[app.id]}
    <div class="card row" class:app-card={view === "cards"}>
      <div class="app-info">
        <strong>{app.name || app.repo.split("/")[1]}</strong>
        <span class="muted small">{app.repo}</span>
        <div class="muted small">
          Installed: {app.installed_version ?? "unknown"}
          {#if app.include_prerelease} · pre-releases{/if}
          {#if app.skipped_version} · skipping {app.skipped_version}{/if}
        </div>
      </div>
      <label class="check small"><input type="checkbox" checked={app.enabled} onchange={() => toggle(app)} /> Enabled</label>
      <button onclick={() => edit(app)} disabled={running}>Edit</button>
      {#if canUninstall(app) || running}
        <button
          class="danger"
          onclick={() => uninstall(app)}
          disabled={!!store.status?.installing}
          title="Remove the installed software from this computer"
        >
          {running ? "Uninstalling…" : "Uninstall"}
        </button>
      {/if}
      <button class="danger" onclick={() => remove(app)} disabled={running} title="Stop tracking; installed software is kept">
        Remove
      </button>
      {#if running}
        <div style="flex-basis: 100%">
          <div class="small">Uninstalling — approve the {elevation} prompt…</div>
          <progress></progress>
          {#if live?.log.length}<pre class="log">{live.log.join("\n")}</pre>{/if}
        </div>
      {:else if done}
        <div style="flex-basis: 100%">
          <div class="small" class:ok={done.ok} class:danger-text={!done.ok}>{done.message}</div>
          {#if done.log.length}<details><summary class="small">Output</summary><pre class="log">{done.log.join("\n")}</pre></details>{/if}
          <button class="link small" onclick={() => delete uninstallResult[app.id]}>Dismiss</button>
        </div>
      {/if}
    </div>
  {:else}
    <p class="muted">Add a repository above, e.g. <code>storytold/wordcraft</code>.</p>
  {/each}
  </div>
{/if}

<style>
  .segmented { display: inline-flex; }
  .segmented button { border-radius: 0; }
  .segmented button:first-child { border-radius: 6px 0 0 6px; }
  .segmented button:last-child { border-radius: 0 6px 6px 0; margin-left: -1px; }
  .segmented button.active { background: var(--accent); color: var(--accent-text); border-color: var(--accent); position: relative; }

  .app-info { flex: 1; min-width: 200px; }

  .app-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
    margin-bottom: 12px;
  }
  .app-grid > p { grid-column: 1 / -1; }
  .app-card { margin-bottom: 0; align-content: flex-start; }
  .app-card .app-info { flex-basis: 100%; min-width: 0; }
  .app-card .app-info > span { display: block; overflow-wrap: anywhere; }
</style>
