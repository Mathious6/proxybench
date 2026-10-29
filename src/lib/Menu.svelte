<script lang="ts" module>
  export type MenuEntry = {
    label: string;
    disabled: boolean;
    tone: "plain" | "danger";
    act: () => void;
  };
</script>

<script lang="ts">
  import { onMount } from "svelte";

  let {
    x,
    y,
    label,
    entries,
    trigger,
    onDismiss,
  }: {
    x: number;
    y: number;
    label: string;
    entries: MenuEntry[];
    trigger: HTMLElement | null;
    onDismiss: () => void;
  } = $props();

  let root = $state<HTMLDivElement | null>(null);

  function items(): HTMLButtonElement[] {
    return [...(root?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];
  }

  function dismiss() {
    trigger?.focus();
    onDismiss();
  }

  function choose(entry: MenuEntry) {
    trigger?.focus();
    onDismiss();
    entry.act();
  }

  onMount(() => {
    const node = root;
    if (!node) {
      return;
    }
    node.style.left = `${Math.max(8, Math.min(x, window.innerWidth - node.offsetWidth - 8))}px`;
    node.style.top = `${Math.max(8, Math.min(y, window.innerHeight - node.offsetHeight - 8))}px`;
    (items()[0] ?? node).focus();
    const outside = (event: PointerEvent) => {
      if (!node.contains(event.target as Node)) {
        onDismiss();
      }
    };
    const blur = () => onDismiss();
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("blur", blur);
    };
  });

  function navigate(event: KeyboardEvent) {
    const list = items();
    const current = list.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape" || event.key === "Tab") {
      event.preventDefault();
      event.stopPropagation();
      dismiss();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      list[(current + step + list.length) % list.length]?.focus();
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      list[event.key === "Home" ? 0 : list.length - 1]?.focus();
    }
  }
</script>

<div
  bind:this={root}
  class="menu"
  style:left="{x}px"
  style:top="{y}px"
  role="menu"
  aria-label={label}
  tabindex="-1"
  onkeydown={navigate}
>
  {#each entries as entry (entry.label)}
    <button
      type="button"
      role="menuitem"
      class={entry.tone}
      disabled={entry.disabled}
      onclick={() => choose(entry)}
    >
      {entry.label}
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 30;
    min-width: 152px;
    padding: 4px;
    border: 1px solid var(--color-edge);
    border-radius: var(--radius-control);
    background: var(--color-raised);
    font-size: var(--text-xs);
  }

  .menu:focus {
    outline: none;
  }

  button {
    display: block;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius-chip);
    background: none;
    color: var(--color-text);
    text-align: left;
  }

  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--color-line);
    outline: none;
  }

  button:disabled {
    opacity: 0.4;
  }

  .danger {
    color: var(--color-bad);
  }
</style>
