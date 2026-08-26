<script lang="ts">
  let {
    x,
    y,
    locked,
    onProbe,
    onExport,
    onRemove,
    onDismiss,
  }: {
    x: number;
    y: number;
    locked: boolean;
    onProbe: () => void;
    onExport: () => void;
    onRemove: () => void;
    onDismiss: () => void;
  } = $props();

  let root = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const node = root;
    if (!node) {
      return;
    }
    const left = Math.min(x, window.innerWidth - node.offsetWidth - 8);
    const top = Math.min(y, window.innerHeight - node.offsetHeight - 8);
    node.style.left = `${Math.max(8, left)}px`;
    node.style.top = `${Math.max(8, top)}px`;
    node.focus();
  });
</script>

<div
  bind:this={root}
  class="fixed z-30 min-w-32 rounded-md border border-line bg-raised py-1 text-xs text-text shadow-none"
  style="left: {x}px; top: {y}px"
  role="menu"
  tabindex="-1"
  onkeydown={(event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onDismiss();
    }
  }}
>
  <button
    type="button"
    class="block w-full px-3 py-1.5 text-left hover:bg-bg disabled:opacity-40"
    disabled={locked}
    role="menuitem"
    onclick={onProbe}
  >
    Probe
  </button>
  <button
    type="button"
    class="block w-full px-3 py-1.5 text-left hover:bg-bg disabled:opacity-40"
    disabled={locked}
    role="menuitem"
    onclick={onExport}
  >
    Export
  </button>
  <button
    type="button"
    class="block w-full px-3 py-1.5 text-left text-bad hover:bg-bg disabled:opacity-40"
    disabled={locked}
    role="menuitem"
    onclick={onRemove}
  >
    Remove
  </button>
</div>
