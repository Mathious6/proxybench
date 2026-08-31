<script lang="ts">
  let { locked, onAycd, onDismiss }: { locked: boolean; onAycd: () => void; onDismiss: () => void } = $props();

  let item = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    item?.focus();
  });
</script>

<div
  class="absolute right-0 top-full z-30 mt-1 min-w-48 rounded-md border border-line bg-raised py-1 text-xs text-text shadow-none"
  role="menu"
  tabindex="-1"
  onkeydown={(event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onDismiss();
    } else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      item?.focus();
    }
  }}
>
  <button
    bind:this={item}
    type="button"
    class="block w-full px-3 py-1.5 text-left hover:bg-bg disabled:opacity-40"
    disabled={locked}
    role="menuitem"
    onclick={onAycd}
  >
    Export for AYCD (.json)
  </button>
</div>
