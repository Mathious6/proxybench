<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    children,
    disabled = false,
    danger = false,
    onclick,
  }: {
    children: Snippet;
    disabled?: boolean;
    danger?: boolean;
    onclick: () => void;
  } = $props();
</script>

<button
  type="button"
  class="block w-full px-3 py-1.5 text-left outline-none hover:bg-bg focus:bg-bg disabled:opacity-40 {danger ? 'text-bad' : 'text-text'}"
  {disabled}
  role="menuitem"
  {onclick}
>
  {@render children()}
</button>
