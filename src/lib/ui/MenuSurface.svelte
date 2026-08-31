<script lang="ts">
  import { onDestroy, tick, type Snippet } from "svelte";

  let {
    children,
    class: className = "",
    trigger = null,
    onDismiss,
  }: {
    children: Snippet;
    class?: string;
    trigger?: HTMLElement | null;
    onDismiss: () => void;
  } = $props();

  let root = $state<HTMLDivElement | null>(null);
  let restored = false;

  function restoreFocus() {
    if (restored) return;
    restored = true;
    trigger?.focus();
  }

  function dismiss() {
    restoreFocus();
    onDismiss();
  }

  $effect(() => {
    void tick().then(() => {
      const first = root?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)');
      (first ?? root)?.focus();
    });
  });

  onDestroy(restoreFocus);

  function moveFocus(event: KeyboardEvent) {
    const items = [...(root?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape") {
      event.preventDefault();
      dismiss();
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      items[event.key === "Home" ? 0 : items.length - 1]?.focus();
    } else if ((event.key === "ArrowDown" || event.key === "ArrowUp") && items.length > 0) {
      event.preventDefault();
      items[(current + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
    }
  }
</script>

<div bind:this={root} class="z-30 rounded-[var(--radius-control)] border border-line bg-raised py-1 text-xs text-text shadow-[0_12px_30px_rgb(0_0_0_/_0.3)] {className}" role="menu" tabindex="-1" onkeydown={moveFocus}>
  {@render children()}
</div>
