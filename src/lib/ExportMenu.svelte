<script lang="ts">
  import MenuItem from "./ui/MenuItem.svelte";
  import MenuSurface from "./ui/MenuSurface.svelte";

  let { locked, trigger, onAycd, onDismiss }: { locked: boolean; trigger: HTMLElement | null; onAycd: () => void; onDismiss: () => void } = $props();
</script>

<MenuSurface class="absolute right-0 top-full mt-1 min-w-48" {trigger} {onDismiss}>
  <MenuItem disabled={locked} onclick={onAycd}>Export for AYCD (.json)</MenuItem>
</MenuSurface>
