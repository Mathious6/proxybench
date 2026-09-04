<script lang="ts">
  import Pill from "./ui/Pill.svelte";

  let { value }: { value: string } = $props();
</script>

<Pill tone="country">{value}</Pill>
