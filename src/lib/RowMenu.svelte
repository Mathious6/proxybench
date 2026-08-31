<script lang="ts">
  import MenuItem from "./ui/MenuItem.svelte";
  import MenuSurface from "./ui/MenuSurface.svelte";

  let {
    x,
    y,
    trigger,
    locked,
    onProbe,
    onExport,
    onExportAycd,
    onRemove,
    onDismiss,
  }: {
    x: number;
    y: number;
    trigger: HTMLElement | null;
    locked: boolean;
    onProbe: () => void;
    onExport: () => void;
    onExportAycd: () => void;
    onRemove: () => void;
    onDismiss: () => void;
  } = $props();

  let root = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const node = root;
    if (!node) return;
    const left = Math.min(x, window.innerWidth - node.offsetWidth - 8);
    const top = Math.min(y, window.innerHeight - node.offsetHeight - 8);
    node.style.left = `${Math.max(8, left)}px`;
    node.style.top = `${Math.max(8, top)}px`;
  });
</script>

<div bind:this={root} class="fixed z-30" style="left: {x}px; top: {y}px">
<MenuSurface class="min-w-32" {trigger} {onDismiss}>
    <MenuItem disabled={locked} onclick={onProbe}>Probe</MenuItem>
    <MenuItem disabled={locked} onclick={onExport}>Export</MenuItem>
    <MenuItem disabled={locked} onclick={onExportAycd}>Export for AYCD (.json)</MenuItem>
    <MenuItem disabled={locked} danger onclick={onRemove}>Remove</MenuItem>
  </MenuSurface>
</div>
