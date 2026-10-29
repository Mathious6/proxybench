<script lang="ts">
  import type { SubnetRow } from "./ipc";
  import HealthDot from "./HealthDot.svelte";
  import TagsCell from "./TagsCell.svelte";
  import { DASH, count, okTone, p50Tone, p95Tone, stamp, stampTitle, timing } from "./format";
  import { healthKey, type Gesture, type SortKey, type View } from "./view.svelte";

  type Column = { key: SortKey; label: string; align: "start" | "end"; tone: "muted" | "faint" };

  let {
    view,
    checking,
    onMenu,
    onTags,
  }: {
    view: View;
    checking: (cidr: string) => boolean;
    onMenu: (cidr: string, x: number, y: number, trigger: HTMLElement) => void;
    onTags: (cidr: string, tags: string[]) => void;
  } = $props();

  const columns: Column[] = [
    { key: "country", label: "Country", align: "start", tone: "muted" },
    { key: "quantity", label: "Qty", align: "end", tone: "muted" },
    { key: "ok", label: "OK", align: "end", tone: "muted" },
    { key: "connectP50", label: "Connect", align: "end", tone: "muted" },
    { key: "connectP95", label: "p95", align: "end", tone: "faint" },
    { key: "ttfbP50", label: "TTFB", align: "end", tone: "muted" },
    { key: "ttfbP95", label: "p95", align: "end", tone: "faint" },
    { key: "lastProbe", label: "Last probe", align: "start", tone: "muted" },
  ];

  const sortNames: Record<SortKey, string> = {
    health: "health",
    cidr: "subnet",
    country: "country",
    quantity: "quantity",
    ok: "OK",
    connectP50: "Connect p50",
    connectP95: "Connect p95",
    ttfbP50: "TTFB p50",
    ttfbP95: "TTFB p95",
    lastProbe: "last probe",
  };

  function ariaSort(key: SortKey): "ascending" | "descending" | undefined {
    if (view.sort?.key !== key) {
      return undefined;
    }
    return view.sort.direction === "asc" ? "ascending" : "descending";
  }

  function arrow(key: SortKey): string {
    if (view.sort?.key !== key) {
      return "";
    }
    return view.sort.direction === "asc" ? "↑" : "↓";
  }

  function interactive(target: EventTarget | null): boolean {
    return target instanceof Element && target.closest("button, input") !== null;
  }

  function gesture(event: MouseEvent | KeyboardEvent): Gesture {
    const additive = event.metaKey || event.ctrlKey;
    if (event.shiftKey) {
      return additive ? "extend" : "range";
    }
    return additive ? "toggle" : "replace";
  }

  function click(event: MouseEvent, cidr: string) {
    if (!interactive(event.target)) {
      view.pick(cidr, gesture(event));
    }
  }

  function key(event: KeyboardEvent, cidr: string) {
    if (interactive(event.target)) {
      return;
    }
    const row = event.currentTarget as HTMLElement;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      view.pick(cidr, view.selected.has(cidr) && gesture(event) === "replace" ? "toggle" : gesture(event));
    } else if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault();
      const bounds = row.getBoundingClientRect();
      onMenu(cidr, bounds.left + 32, bounds.bottom, row);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = event.key === "ArrowDown" ? row.nextElementSibling : row.previousElementSibling;
      (next as HTMLElement | null)?.focus();
    }
  }

  function tone(row: SubnetRow, key: SortKey): string {
    const probe = row.probe;
    if (key === "country") {
      return row.country ? "muted" : "faint";
    }
    if (key === "quantity") {
      return "muted";
    }
    if (key === "lastProbe") {
      return "faint";
    }
    if (!probe) {
      return "faint";
    }
    if (key === "ok") {
      return okTone(probe.ok, probe.tested);
    }
    if (key === "connectP50" || key === "ttfbP50") {
      return p50Tone(probe[key]);
    }
    if (key === "connectP95" || key === "ttfbP95") {
      return p95Tone(probe[key]);
    }
    return "plain";
  }

  function text(row: SubnetRow, key: SortKey): string {
    const probe = row.probe;
    switch (key) {
      case "country":
        return row.country ?? DASH;
      case "quantity":
        return count(row.quantity);
      case "ok":
        return probe ? count(probe.ok) : DASH;
      case "lastProbe":
        return stamp(probe?.at ?? null);
      case "connectP50":
      case "connectP95":
      case "ttfbP50":
      case "ttfbP95":
        return timing(probe?.[key] ?? null);
      default:
        return "";
    }
  }
</script>

<table>
  <colgroup>
    <col style:width="192px" />
    <col style:width="68px" />
    <col style:width="56px" />
    <col style:width="56px" />
    <col style:width="76px" />
    <col style:width="64px" />
    <col style:width="72px" />
    <col style:width="64px" />
    <col style:width="112px" />
    <col />
  </colgroup>
  <thead>
    <tr>
      <th class="subnet" aria-sort={ariaSort(view.sort?.key === "health" ? "health" : "cidr")}>
        <div class="subnet-head">
          <button type="button" class="sort health" aria-label="Sort by health" title="Health" onclick={() => view.toggleSort("health")}>
            <span class="ring" class:active={view.sort?.key === "health"}></span><span class="arrow">{arrow("health")}</span>
          </button>
          <button type="button" class="sort" aria-label="Sort by subnet" onclick={() => view.toggleSort("cidr")}>
            Subnet<span class="arrow">{arrow("cidr")}</span>
          </button>
        </div>
      </th>
      {#each columns as column (column.key)}
        <th class={column.align} aria-sort={ariaSort(column.key)}>
          <button type="button" class="sort {column.tone}" aria-label="Sort by {sortNames[column.key]}" onclick={() => view.toggleSort(column.key)}>
            {#if column.align === "end"}<span class="arrow">{arrow(column.key)}</span>{/if}{column.label}{#if column.align === "start"}<span class="arrow">{arrow(column.key)}</span>{/if}
          </button>
        </th>
      {/each}
      <th class="tags-head">Tags</th>
    </tr>
  </thead>
  <tbody>
    {#each view.visible as row (row.cidr)}
      {@const selected = view.selected.has(row.cidr)}
      <tr
        class:selected
        tabindex="0"
        aria-selected={selected}
        onclick={(event) => click(event, row.cidr)}
        oncontextmenu={(event) => {
          event.preventDefault();
          onMenu(row.cidr, event.clientX, event.clientY, event.currentTarget);
        }}
        onkeydown={(event) => key(event, row.cidr)}
      >
        <td class="subnet">
          <span class="subnet-cell">
            <HealthDot state={checking(row.cidr) ? "checking" : healthKey(row)} />
            <span>{row.cidr}</span>
          </span>
        </td>
        {#each columns as column (column.key)}
          <td
            class="{column.align} {tone(row, column.key)}"
            title={column.key === "lastProbe" ? stampTitle(row.probe?.at ?? null) : undefined}
          >
            {text(row, column.key)}
          </td>
        {/each}
        <td class="tags">
          <TagsCell cidr={row.cidr} tags={row.tags} onChange={onTags} />
        </td>
      </tr>
    {/each}
  </tbody>
</table>

<style>
  table {
    width: 100%;
    table-layout: fixed;
    border-collapse: separate;
    border-spacing: 0;
  }

  th,
  td {
    padding: 0 12px 0 0;
    overflow: hidden;
    border-bottom: 1px solid var(--color-line);
    white-space: nowrap;
  }

  th {
    height: var(--height-head);
    font-size: var(--text-xs);
    font-weight: 400;
  }

  td {
    height: var(--height-row);
  }

  tbody tr:last-child td {
    border-bottom-color: transparent;
  }

  .start {
    padding-left: 12px;
    padding-right: 0;
    text-align: left;
  }

  .end {
    text-align: right;
  }

  th.subnet,
  td.subnet {
    padding-left: var(--gutter);
  }

  .tags,
  .tags-head {
    padding: 0 var(--gutter) 0 12px;
    color: var(--color-faint);
    text-align: left;
  }

  .subnet-head,
  .subnet-cell {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .sort {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-chip);
    background: none;
    color: var(--color-muted);
  }

  .sort.faint {
    color: var(--color-faint);
  }

  .sort:hover {
    color: var(--color-text);
  }

  .arrow {
    color: var(--color-accent);
  }

  .arrow:empty {
    display: none;
  }

  .sort.health {
    justify-content: center;
    gap: 2px;
    min-width: 16px;
    margin: 0 -5px;
  }

  .ring {
    width: 6px;
    height: 6px;
    border: 1px solid var(--color-faint);
    border-radius: 50%;
  }

  .sort.health:hover .ring,
  .ring.active {
    border-color: var(--color-accent);
  }

  tbody tr:hover td {
    background: var(--color-surface);
  }

  tbody tr.selected td {
    background: var(--color-raised);
  }

  tbody tr.selected td.subnet {
    box-shadow: inset 2px 0 0 var(--color-accent);
  }

  tbody tr:focus-visible {
    outline: none;
  }

  tbody tr:focus-visible td {
    box-shadow:
      inset 0 1px 0 var(--color-accent),
      inset 0 -1px 0 var(--color-accent);
  }

  tbody tr:focus-visible td.subnet {
    box-shadow:
      inset 1px 0 0 var(--color-accent),
      inset 0 1px 0 var(--color-accent),
      inset 0 -1px 0 var(--color-accent);
  }

  tbody tr.selected:focus-visible td.subnet {
    box-shadow:
      inset 2px 0 0 var(--color-accent),
      inset 0 1px 0 var(--color-accent),
      inset 0 -1px 0 var(--color-accent);
  }

  tbody tr:focus-visible td.tags {
    box-shadow:
      inset -1px 0 0 var(--color-accent),
      inset 0 1px 0 var(--color-accent),
      inset 0 -1px 0 var(--color-accent);
  }

  .plain {
    color: var(--color-text);
  }

  .muted {
    color: var(--color-muted);
  }

  .faint {
    color: var(--color-faint);
  }

  .good {
    color: var(--color-good);
  }

  .bad {
    color: var(--color-bad);
  }

  td.subnet {
    color: var(--color-text);
  }
</style>
