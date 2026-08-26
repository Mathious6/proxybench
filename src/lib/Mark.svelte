<script lang="ts">
  let { value }: { value: string } = $props();
</script>

<span class="inline-flex h-[22px] max-w-full items-center truncate rounded-md border border-line bg-raised px-1.5 font-mono text-xs text-text">
  {value}
</span>
