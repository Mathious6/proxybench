<script lang="ts">
  import type { Update } from "@tauri-apps/plugin-updater";
  import { checkForUpdate, installUpdate, type UpdateProgress } from "$lib/update";

  let {
    disabled,
    onBusyChange,
    onNotice,
  }: {
    disabled: boolean;
    onBusyChange: (busy: boolean) => void;
    onNotice: (message: string, failed: boolean) => void;
  } = $props();

  let checking = $state(false);
  let installing = $state(false);
  let update = $state<Update | null>(null);
  let progress = $state<UpdateProgress>({ downloaded: 0, total: null });

  const progressLabel = $derived.by(() => {
    if (!installing) {
      return "";
    }
    if (!progress.total) {
      return "Updating…";
    }
    return `Updating ${Math.min(100, Math.round((progress.downloaded / progress.total) * 100))}%`;
  });

  async function findUpdate() {
    if (disabled || checking || installing) {
      return;
    }
    checking = true;
    onBusyChange(true);
    onNotice("", false);
    try {
      update = await checkForUpdate();
      if (!update) {
        onNotice("Up to date", false);
      }
    } catch {
      onNotice("Update check failed", true);
    } finally {
      checking = false;
      onBusyChange(false);
    }
  }

  async function applyUpdate() {
    if (!update || disabled || checking || installing) {
      return;
    }
    installing = true;
    onBusyChange(true);
    onNotice("", false);
    try {
      await installUpdate(update, (next) => {
        progress = next;
      });
    } catch {
      onNotice("Update failed. Try again.", true);
      installing = false;
      onBusyChange(false);
    }
  }
</script>

{#if update && !installing}
  <button
    type="button"
    class="h-7 shrink-0 rounded-md border border-line px-3 text-xs text-muted hover:text-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
    disabled={disabled || checking}
    onclick={applyUpdate}
  >
    Update to v{update.version}
  </button>
{:else if installing}
  <span class="shrink-0 text-xs tabular-nums text-muted">{progressLabel}</span>
{:else}
  <button
    type="button"
    class="h-7 shrink-0 rounded-md border border-line px-3 text-xs text-faint hover:text-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
    disabled={disabled || checking}
    onclick={findUpdate}
  >
    {checking ? "Checking…" : "Check for updates"}
  </button>
{/if}
