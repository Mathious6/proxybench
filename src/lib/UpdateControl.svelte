<script lang="ts">
  import { ipc, type AvailableUpdate } from "./ipc";

  type Phase = { kind: "idle" } | { kind: "checking" } | { kind: "ready"; update: AvailableUpdate } | { kind: "installing"; fraction: number | null };

  let {
    disabled,
    onBusy,
    onInfo,
    onError,
  }: {
    disabled: boolean;
    onBusy: (busy: boolean) => void;
    onInfo: (message: string) => void;
    onError: (message: string) => void;
  } = $props();

  let phase = $state<Phase>({ kind: "idle" });

  const label = $derived.by(() => {
    switch (phase.kind) {
      case "checking":
        return "Checking for updates";
      case "ready":
        return `Install v${phase.update.version}`;
      case "installing":
        return phase.fraction === null ? "Installing update" : `Installing update, ${Math.round(phase.fraction * 100)}%`;
      default:
        return "Check for updates";
    }
  });

  async function find() {
    phase = { kind: "checking" };
    onBusy(true);
    try {
      const update = await ipc.findUpdate();
      phase = update ? { kind: "ready", update } : { kind: "idle" };
      onInfo(update ? `Version ${update.version} available` : "Up to date");
    } catch {
      phase = { kind: "idle" };
      onError("Update check failed");
    } finally {
      onBusy(false);
    }
  }

  async function install(update: AvailableUpdate) {
    phase = { kind: "installing", fraction: null };
    onBusy(true);
    try {
      await update.install((fraction) => {
        phase = { kind: "installing", fraction };
        onInfo(fraction === null ? "Updating…" : `Updating ${Math.round(fraction * 100)}%`);
      });
    } catch {
      phase = { kind: "ready", update };
      onBusy(false);
      onError("Update failed. Try again.");
    }
  }
</script>

<button
  type="button"
  class="link update"
  class:ready={phase.kind === "ready"}
  class:spinning={phase.kind === "checking" || phase.kind === "installing"}
  disabled={disabled || phase.kind === "checking" || phase.kind === "installing"}
  aria-label={label}
  aria-busy={phase.kind === "checking" || phase.kind === "installing"}
  title={label}
  onclick={() => (phase.kind === "ready" ? install(phase.update) : find())}
>
  <span class="glyph" aria-hidden="true">{phase.kind === "ready" ? "↓" : "↻"}</span>
</button>

<style>
  .update {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
  }

  .ready {
    color: var(--color-accent);
  }

  .glyph {
    display: inline-block;
  }

  @media (prefers-reduced-motion: no-preference) {
    .spinning .glyph {
      animation: spin 1s linear infinite;
    }
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
