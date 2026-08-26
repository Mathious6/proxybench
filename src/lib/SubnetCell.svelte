<script lang="ts">
  import Mark from "./Mark.svelte";

  let { cidr }: { cidr: string } = $props();
</script>

<Mark value={cidr} />
