<script lang="ts">
  let {
    cidr,
    tags,
    onChange,
  }: {
    cidr: string;
    tags: string[];
    onChange: (cidr: string, tags: string[]) => void;
  } = $props();

  let draft = $state("");

  function add() {
    const value = draft.trim();
    if (value) {
      onChange(cidr, [...tags, value]);
    }
    draft = "";
  }
</script>

<div class="tags" title={tags.join(", ") || undefined}>
  {#each tags as tag (tag)}
    <span class="chip">
      <span class="name">{tag}</span>
      <button type="button" class="remove" aria-label="Remove tag {tag}" onclick={() => onChange(cidr, tags.filter((item) => item !== tag))}>×</button>
    </span>
  {/each}
  <input
    class="add"
    class:filled={draft !== ""}
    placeholder="add"
    aria-label="Add tag to {cidr}"
    spellcheck="false"
    autocomplete="off"
    bind:value={draft}
    onkeydown={(event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        add();
      } else if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        draft = "";
        event.currentTarget.blur();
      }
    }}
  />
</div>

<style>
  .tags {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    overflow: hidden;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    min-width: 0;
    max-width: 128px;
    height: var(--height-chip);
    padding: 0 6px;
    border: 1px solid var(--color-edge);
    border-radius: var(--radius-chip);
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .remove {
    display: none;
    flex-shrink: 0;
    width: 14px;
    margin-right: -4px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--color-faint);
  }

  .remove:hover {
    color: var(--color-text);
  }

  .add {
    flex-shrink: 0;
    width: 72px;
    height: var(--height-chip);
    padding: 0 4px;
    border: 1px solid transparent;
    border-radius: var(--radius-chip);
    background: transparent;
    color: var(--color-text);
    font-size: var(--text-xs);
    opacity: 0;
  }

  .add::placeholder {
    color: var(--color-faint);
  }

  .add:focus {
    border-color: var(--color-accent);
    outline: none;
  }

  .add:focus,
  .add.filled,
  :global(tr:hover) .add,
  :global(tr:focus-within) .add {
    opacity: 1;
  }

  :global(tr:hover) .remove,
  :global(tr:focus-within) .remove {
    display: inline-block;
  }
</style>
