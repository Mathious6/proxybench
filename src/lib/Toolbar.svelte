<script lang="ts">
  import { plural } from "./format";

  let {
    total,
    selected,
    query,
    filterField = $bindable(null),
    canImport,
    canExport,
    probeLabel,
    canProbe,
    onClear,
    onQuery,
    onOpen,
    onExport,
    onProbe,
  }: {
    total: number;
    selected: number;
    query: string;
    filterField?: HTMLInputElement | null;
    canImport: boolean;
    canExport: boolean;
    probeLabel: string;
    canProbe: boolean;
    onClear: () => void;
    onQuery: (query: string) => void;
    onOpen: () => void;
    onExport: (trigger: HTMLElement) => void;
    onProbe: () => void;
  } = $props();
</script>

<header class="toolbar">
  <div class="title">
    <h1>proxybench</h1>
    {#if selected > 0}
      <span class="selected">{selected} selected</span>
      <button type="button" class="link" title="Clear selection (Esc)" onclick={onClear}>Clear</button>
    {:else}
      <span class="count">{plural(total, "subnet", "subnets")}</span>
    {/if}
  </div>
  <div class="actions">
    <input
      bind:this={filterField}
      class="field filter"
      type="search"
      placeholder="Filter"
      aria-label="Filter subnets"
      spellcheck="false"
      autocomplete="off"
      disabled={total === 0}
      value={query}
      oninput={(event) => onQuery(event.currentTarget.value)}
      onkeydown={(event) => {
        if (event.key === "Escape" && query) {
          event.stopPropagation();
          onQuery("");
        }
      }}
    />
    <button type="button" class="button" disabled={!canImport} onclick={onOpen}>Open files</button>
    <button
      type="button"
      class="button"
      aria-haspopup="menu"
      disabled={!canExport}
      onclick={(event) => onExport(event.currentTarget)}
    >
      Export<span class="caret" aria-hidden="true">▾</span>
    </button>
    <button type="button" class="button primary probe" disabled={!canProbe} onclick={onProbe}>{probeLabel}</button>
  </div>
</header>

<style>
  .toolbar {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    height: calc(var(--height-toolbar) + 1px);
    padding: 0 var(--gutter);
    border-bottom: 1px solid var(--color-line);
  }

  .title {
    display: flex;
    align-items: baseline;
    gap: 12px;
    min-width: 0;
  }

  h1 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .count {
    color: var(--color-faint);
    font-size: var(--text-xs);
  }

  .selected {
    color: var(--color-accent);
    font-size: var(--text-xs);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .filter {
    width: 200px;
  }

  .filter::-webkit-search-cancel-button {
    display: none;
  }

  .caret {
    color: var(--color-faint);
    font-size: var(--text-2xs);
  }

  .probe {
    min-width: 96px;
  }
</style>
