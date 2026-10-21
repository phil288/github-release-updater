<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, errorText } from "./lib/api";
  import { emptyProgress, store } from "./lib/store.svelte";
  import type { FinishedEvent, Page, ProgressEvent, Status } from "./lib/types";
  import Updates from "./components/Updates.svelte";
  import Apps from "./components/Apps.svelte";
  import SettingsPage from "./components/Settings.svelte";

  let loadError = $state<string | null>(null);

  const tabs: { id: Page; label: string }[] = [
    { id: "updates", label: "Updates" },
    { id: "apps", label: "Applications" },
    { id: "settings", label: "Settings" },
  ];

  function onProgress(e: ProgressEvent) {
    const p = (store.progress[e.app_id] ??= emptyProgress());
    switch (e.stage) {
      case "downloading":
        p.stage = "downloading";
        p.downloaded = e.downloaded;
        p.total = e.total;
        break;
      case "verifying":
      case "installing":
        p.stage = e.stage;
        break;
      case "log":
        p.log.push(e.line);
        if (p.log.length > 500) p.log.splice(0, p.log.length - 500);
        break;
    }
  }

  onMount(() => {
    const unlisten = [
      listen<Page>("navigate", (e) => (store.page = e.payload)),
      listen<Status>("status-changed", (e) => (store.status = e.payload)),
      listen<ProgressEvent>("install-progress", (e) => onProgress(e.payload)),
      listen<FinishedEvent>("install-finished", (e) => {
        const p = (store.progress[e.payload.app_id] ??= emptyProgress());
        p.stage = e.payload.stage;
        p.message = e.payload.message;
        // An automatic install may have changed the installed version.
        api.getSettings().then((s) => (store.settings = s), () => {});
      }),
    ];
    (async () => {
      try {
        [store.settings, store.status, store.platform] = await Promise.all([
          api.getSettings(),
          api.getStatus(),
          api.platform(),
        ]);
      } catch (e) {
        loadError = errorText(e);
      }
    })();
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });

  const pendingCount = $derived(store.status?.pending.length ?? 0);
</script>

<nav>
  <strong class="brand">GitHub Release Updater</strong>
  {#each tabs as tab}
    <button class:active={store.page === tab.id} onclick={() => (store.page = tab.id)}>
      {tab.label}
      {#if tab.id === "updates" && pendingCount > 0}<span class="count">{pendingCount}</span>{/if}
    </button>
  {/each}
</nav>

<main>
  {#if loadError}
    <div class="notice error">{loadError}</div>
  {:else if !store.settings}
    <p class="muted">Loading…</p>
  {:else if store.page === "updates"}
    <Updates />
  {:else if store.page === "apps"}
    <Apps />
  {:else}
    <SettingsPage />
  {/if}
</main>

<style>
  nav {
    display: flex;
    gap: 4px;
    align-items: center;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    position: sticky;
    top: 0;
    z-index: 1;
  }
  .brand { margin-right: 16px; }
  nav button { border: none; background: none; padding: 8px 12px; }
  nav button.active { background: var(--bg); font-weight: 600; }
  .count {
    background: var(--accent);
    color: var(--accent-text);
    border-radius: 999px;
    padding: 0 7px;
    margin-left: 4px;
    font-size: 0.8em;
  }
  main { padding: 16px; max-width: 980px; margin: 0 auto; }
</style>
