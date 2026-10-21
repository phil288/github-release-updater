<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, errorText } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import type { Preview, TrackedApp } from "../lib/types";

  let { app, isNew, onsave, oncancel }: {
    app: TrackedApp;
    isNew: boolean;
    onsave: (app: TrackedApp) => void;
    oncancel: () => void;
  } = $props();

  // Local editable copy of the initial prop; empty strings become null on save.
  let draft = $state(untrack(() => ({
    ...app,
    name: app.name ?? "",
    package_name: app.package_name ?? "",
    windows_display_name: app.windows_display_name ?? "",
    installed_version: app.installed_version ?? "",
    linux: app.asset_pattern.linux ?? "",
    windows: app.asset_pattern.windows ?? "",
  })));
  let preview = $state<Preview | null | undefined>(undefined);
  let previewing = $state(false);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);

  const nul = (s: string) => (s.trim() === "" ? null : s.trim());

  function build(): TrackedApp {
    return {
      id: draft.id,
      repo: draft.repo,
      name: nul(draft.name),
      enabled: draft.enabled,
      include_prerelease: draft.include_prerelease,
      asset_pattern: { linux: nul(draft.linux), windows: nul(draft.windows) },
      package_name: nul(draft.package_name),
      windows_display_name: nul(draft.windows_display_name),
      installed_version: nul(draft.installed_version),
      skipped_version: draft.skipped_version,
    };
  }

  async function runPreview() {
    error = null;
    previewing = true;
    try {
      preview = await api.previewApp(build());
    } catch (e) {
      error = errorText(e);
    } finally {
      previewing = false;
    }
  }

  async function detect() {
    error = null;
    message = null;
    try {
      const v = await api.detectInstalled(build());
      if (v) {
        draft.installed_version = v;
        message = `Detected installed version ${v}.`;
      } else {
        message = "Not found on this system. Check the package / display name.";
      }
    } catch (e) {
      error = errorText(e);
    }
  }

  const os = $derived(store.platform?.os ?? "linux");

  // Show the asset selection right away for new apps.
  onMount(() => {
    if (isNew) runPreview();
  });
</script>

<div class="card">
  <div class="row" style="margin-bottom: 12px">
    <h3>{isNew ? "Add" : "Edit"} {draft.repo}</h3>
  </div>
  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice">{message}</div>{/if}

  <div class="grid2">
    <label class="field"><span>Display name</span>
      <input type="text" bind:value={draft.name} placeholder={draft.repo.split("/")[1]} /></label>
    <label class="field"><span>Installed version (empty = unknown, offer latest)</span>
      <div class="row" style="flex-wrap: nowrap">
        <input type="text" bind:value={draft.installed_version} placeholder="e.g. 0.7.0" />
        <button type="button" onclick={detect}>Detect</button>
      </div></label>

    <label class="field"><span>Debian package name (Ubuntu version detection)</span>
      <input type="text" bind:value={draft.package_name} placeholder="filled in after first install" /></label>
    <label class="field"><span>Windows program name (Programs and Features)</span>
      <input type="text" bind:value={draft.windows_display_name} placeholder={draft.name || draft.repo.split("/")[1]} /></label>

    <label class="field"><span>Ubuntu asset pattern (regex, optional)</span>
      <input type="text" bind:value={draft.linux} placeholder="automatic: .deb for this CPU" /></label>
    <label class="field"><span>Windows asset pattern (regex, optional)</span>
      <input type="text" bind:value={draft.windows} placeholder="automatic: .msi for this CPU" /></label>
  </div>

  <div class="row" style="margin-top: 12px; gap: 20px">
    <label class="check"><input type="checkbox" bind:checked={draft.enabled} /> Enabled</label>
    <label class="check"><input type="checkbox" bind:checked={draft.include_prerelease} /> Include pre-releases</label>
    {#if draft.skipped_version}
      <span class="small muted">Skipping {draft.skipped_version}
        <button type="button" class="link" onclick={() => (draft.skipped_version = null)}>undo</button></span>
    {/if}
  </div>

  <div class="card" style="margin-top: 12px; background: var(--bg)">
    <div class="row">
      <strong>Asset on this {os === "windows" ? "Windows" : "Ubuntu"} machine</strong>
      <span class="spacer"></span>
      <button type="button" onclick={runPreview} disabled={previewing}>{previewing ? "Checking…" : "Test match"}</button>
    </div>
    {#if preview === null}
      <p class="muted small">This repository has no published release.</p>
    {:else if preview}
      <p class="small">
        Latest release <strong>{preview.tag}</strong>:
        {#if preview.selected}
          <code>{preview.selected}</code>
          <span class={preview.checksum ? "ok" : "warn"}>
            · {preview.checksum ? `checksum from ${preview.checksum}` : "no checksum published"}</span>
        {:else}
          <span class="danger-text">no matching asset</span>
        {/if}
      </p>
      {#if preview.installer_assets.length > 1 || !preview.selected}
        <p class="muted small">Candidates: {preview.installer_assets.join(", ") || "none"}</p>
      {/if}
    {/if}
  </div>

  <div class="row" style="margin-top: 12px">
    <button class="primary" onclick={() => onsave(build())}>{isNew ? "Add application" : "Save"}</button>
    <button onclick={oncancel}>Cancel</button>
  </div>
</div>
