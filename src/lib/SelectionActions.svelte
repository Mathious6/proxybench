<script lang="ts">
  import Button from "./ui/Button.svelte";
  import MenuItem from "./ui/MenuItem.svelte";
  import MenuSurface from "./ui/MenuSurface.svelte";

  let { locked, onExport, onExportAycd, onProbe }: { locked: boolean; onExport: () => void; onExportAycd: () => void; onProbe: () => void } = $props();

  let menuOpen = $state(false);
  let split = $state<HTMLDivElement | null>(null);
  let trigger = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    if (!menuOpen) return;
    function dismiss(event: MouseEvent) {
      if (split && event.target instanceof Node && !split.contains(event.target)) menuOpen = false;
    }
    window.addEventListener("click", dismiss);
    return () => window.removeEventListener("click", dismiss);
  });

  function closeMenu() {
    menuOpen = false;
  }

  function exportText() {
    closeMenu();
    onExport();
  }

  function probe() {
    closeMenu();
    onProbe();
  }
</script>

<div class="flex items-center gap-1.5">
  <div bind:this={split} class="relative flex">
    <Button variant="outline" joined="start" disabled={locked} onclick={exportText}>Export</Button>
    <Button bind:ref={trigger} variant="outline" size="narrowIcon" joined="end" aria-label="More export options" aria-expanded={menuOpen} aria-haspopup="menu" disabled={locked} onclick={() => (menuOpen = !menuOpen)}>▾</Button>
    {#if menuOpen}
      <MenuSurface class="absolute right-0 top-full mt-1 min-w-48" {trigger} onDismiss={closeMenu}>
        <MenuItem disabled={locked} onclick={() => { closeMenu(); onExportAycd(); }}>Export for AYCD (.json)</MenuItem>
      </MenuSurface>
    {/if}
  </div>
  <Button variant="primary" disabled={locked} onclick={probe}>Probe selected</Button>
</div>
