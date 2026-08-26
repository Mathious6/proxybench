<script lang="ts">
  let {
    cidr,
    tags,
    draft,
    locked,
    onAdd,
    onRemove,
  }: {
    cidr: string;
    tags: string[];
    draft: Record<string, string>;
    locked: boolean;
    onAdd: (cidr: string) => void;
    onRemove: (cidr: string, tag: string) => void;
  } = $props();
</script>

<div class="flex h-full max-h-8 items-center gap-1 overflow-hidden">
  <div
    class="chip-strip flex min-w-0 flex-1 items-center gap-1 overflow-x-auto whitespace-nowrap"
    title={tags.join(", ")}
  >
    {#each tags as tag}
      <button
        type="button"
        class="inline-flex h-[22px] max-w-[104px] shrink-0 items-center rounded-md border border-line bg-raised px-1.5 font-mono text-xs text-text hover:text-muted disabled:opacity-40"
        disabled={locked}
        title={tag}
        onclick={() => onRemove(cidr, tag)}
      >
        <span class="truncate">{tag}</span>
        <span class="shrink-0">&nbsp;×</span>
      </button>
    {/each}
  </div>
  <input
    class="h-6 w-20 shrink-0 rounded-md bg-transparent px-1 text-xs placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
    placeholder="add"
    disabled={locked}
    bind:value={draft[cidr]}
    onkeydown={(event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        onAdd(cidr);
      }
      if (event.key === "Escape") {
        event.preventDefault();
        draft[cidr] = "";
        event.currentTarget.blur();
      }
    }}
  />
</div>
