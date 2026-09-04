<script lang="ts">
  import { onDestroy, tick, type Snippet } from "svelte";

  let { children, label, onDismiss }: { children: Snippet; label: string; onDismiss: () => void } = $props();

  let root = $state<HTMLDivElement | null>(null);
  const previousFocus = typeof document === "undefined" ? null : document.activeElement as HTMLElement | null;
  let restored = false;

  function restoreFocus() {
    if (restored) return;
    restored = true;
    previousFocus?.focus();
  }

  function dismiss() {
    restoreFocus();
    onDismiss();
  }

  $effect(() => {
    void tick().then(() => root?.querySelector<HTMLElement>("input, button, [tabindex]:not([tabindex='-1'])")?.focus());
  });

  onDestroy(restoreFocus);

  function trapFocus(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      dismiss();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = [...(root?.querySelectorAll<HTMLElement>('a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])') ?? [])];
    if (focusable.length === 0) {
      event.preventDefault();
      root?.focus();
      return;
    }
    const current = focusable.indexOf(document.activeElement as HTMLElement);
    if (event.shiftKey && (current <= 0 || document.activeElement === root)) {
      event.preventDefault();
      focusable[focusable.length - 1]?.focus();
    } else if (!event.shiftKey && current === focusable.length - 1) {
      event.preventDefault();
      focusable[0]?.focus();
    }
  }
</script>

<div class="absolute inset-0 z-20 flex items-center justify-center bg-bg/85 px-6" role="presentation" onkeydown={trapFocus}>
  <div bind:this={root} class="w-full max-w-md rounded-[var(--radius-surface)] border border-line bg-raised p-5 shadow-[0_20px_50px_rgb(0_0_0_/_0.35)]" role="dialog" aria-modal="true" aria-label={label} tabindex="-1">
    {@render children()}
  </div>
</div>
