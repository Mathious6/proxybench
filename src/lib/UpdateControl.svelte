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
  let announcement = $state("");

  const label = $derived.by(() => {
    if (checking) {
      return "Checking for updates…";
    }
    if (installing) {
      if (!progress.total) {
        return "Installing update…";
      }
      return `Installing update, ${Math.min(100, Math.round((progress.downloaded / progress.total) * 100))}%`;
    }
    if (update) {
      return `Install v${update.version}`;
    }
    return "Check for updates";
  });

  async function findUpdate() {
    if (disabled || checking || installing) {
      return;
    }
    checking = true;
    onBusyChange(true);
    onNotice("", false);
    announcement = "Checking for updates";
    try {
      update = await checkForUpdate();
      if (!update) {
        onNotice("Up to date", false);
        announcement = "Up to date";
      } else {
        onNotice(`Version ${update.version} available`, false);
        announcement = `Version ${update.version} available`;
      }
    } catch {
      onNotice("Update check failed", true);
      announcement = "Update check failed";
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
    announcement = `Installing version ${update.version}`;
    try {
      await installUpdate(update, (next) => {
        progress = next;
        if (next.total) {
          onNotice(
            `Updating ${Math.min(100, Math.round((next.downloaded / next.total) * 100))}%`,
            false,
          );
        } else {
          onNotice("Updating…", false);
        }
      });
    } catch {
      onNotice("Update failed. Try again.", true);
      announcement = "Update failed. Try again.";
      installing = false;
      onBusyChange(false);
    }
  }
</script>

<button
  type="button"
  class="relative flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-sm {update
    ? 'text-accent'
    : 'text-faint hover:text-text'} focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
  disabled={disabled || checking || installing}
  aria-label={label}
  aria-busy={checking || installing}
  title={label}
  onclick={() => (update ? applyUpdate() : findUpdate())}
>
  <span class={checking || installing ? "animate-spin" : ""} aria-hidden="true">
    {update && !installing ? "↓" : "↻"}
  </span>
  {#if update && !installing}
    <span class="absolute right-0 top-0 h-1 w-1 rounded-full bg-accent" aria-hidden="true"></span>
  {/if}
</button>
<span class="sr-only" role="status" aria-live="polite">{announcement}</span>
