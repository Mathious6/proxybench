<script lang="ts">
  import {
    columnFilteringFeature,
    createColumnHelper,
    createFilteredRowModel,
    createPaginatedRowModel,
    createSortedRowModel,
    createTable,
    filterFn_includesString,
    FlexRender,
    renderComponent,
    rowPaginationFeature,
    rowSortingFeature,
    sortFn_alphanumeric,
    tableFeatures,
  } from "@tanstack/svelte-table";
  import { untrack } from "svelte";
  import Mark from "./Mark.svelte";
  import RowMenu from "./RowMenu.svelte";
  import SubnetCell from "./SubnetCell.svelte";
  import TagsCell from "./TagsCell.svelte";
  import { okTone, timingTone, toneClass, type TimingKey } from "./emphasis";
  import { dash, empty, lastProbe, lastProbeTitle, timing } from "./format";
  import type { SubnetRow } from "./import";

  const PAGE_SIZE = 15;

  let {
    rows,
    selectedCidrs,
    draft,
    locked,
    filtering,
    pageReset,
    onFilterCount,
    onSelectionChange,
    onPageChange,
    onAddTag,
    onRemoveTag,
    onProbeSubnet,
    onExportSubnet,
    onExportAycdSubnet,
    onRemoveSubnet,
  }: {
    rows: SubnetRow[];
    selectedCidrs: Set<string>;
    draft: Record<string, string>;
    locked: boolean;
    filtering: boolean;
    pageReset: number;
    onFilterCount: (count: number) => void;
    onSelectionChange: (cidrs: Set<string>) => void;
    onPageChange: (page: {
      label: string;
      canPrevious: boolean;
      canNext: boolean;
      previous: () => void;
      next: () => void;
    }) => void;
    onAddTag: (cidr: string) => void;
    onRemoveTag: (cidr: string, tag: string) => void;
    onProbeSubnet: (cidrs: string[]) => void;
    onExportSubnet: (cidrs: string[]) => void;
    onExportAycdSubnet: (cidrs: string[]) => void;
    onRemoveSubnet: (cidr: string) => void;
  } = $props();

  const features = tableFeatures({
    rowSortingFeature,
    columnFilteringFeature,
    rowPaginationFeature,
    sortedRowModel: createSortedRowModel(),
    filteredRowModel: createFilteredRowModel(),
    paginatedRowModel: createPaginatedRowModel(),
    sortFns: { alphanumeric: sortFn_alphanumeric },
    filterFns: { includesString: filterFn_includesString },
  });

  const helper = createColumnHelper<typeof features, SubnetRow>();

  const columns = helper.columns([
    helper.accessor("cidr", {
      header: "Subnet",
      filterFn: "includesString",
      cell: (info) =>
        renderComponent(SubnetCell, {
          cidr: info.getValue(),
        }),
    }),
    helper.accessor((row) => row.country ?? "", {
      id: "country",
      header: "Country",
      filterFn: "includesString",
      cell: (info) => {
        const value = info.getValue();
        if (!value) {
          return "—";
        }
        return renderComponent(Mark, { value });
      },
    }),
    helper.accessor("quantity", {
      header: "Qty",
      filterFn: numericFilter,
      cell: (info) => dash(info.getValue()),
    }),
    helper.accessor("ok", {
      header: "OK",
      filterFn: numericFilter,
      cell: (info) => dash(info.getValue()),
    }),
    helper.accessor("lastRunAt", {
      header: "Last probe",
      filterFn: (row, columnId, filterValue) => {
        const query = String(filterValue ?? "").trim().toLowerCase();
        if (!query) {
          return true;
        }
        const value = row.getValue<number | null>(columnId);
        return (
          lastProbe(value).toLowerCase().includes(query) ||
          lastProbeTitle(value).toLowerCase().includes(query)
        );
      },
      cell: (info) => lastProbe(info.getValue()),
    }),
    helper.accessor("connectP50", {
      header: "p50",
      filterFn: numericFilter,
      cell: (info) => timing(info.getValue()),
    }),
    helper.accessor("connectP95", {
      header: "p95",
      filterFn: numericFilter,
      cell: (info) => timing(info.getValue()),
    }),
    helper.accessor("ttfbP50", {
      header: "p50",
      filterFn: numericFilter,
      cell: (info) => timing(info.getValue()),
    }),
    helper.accessor("ttfbP95", {
      header: "p95",
      filterFn: numericFilter,
      cell: (info) => timing(info.getValue()),
    }),
    helper.accessor("tags", {
      header: "Tags",
      sortFn: (rowA, rowB, columnId) => {
        const a = (rowA.getValue<string[]>(columnId) ?? []).join(" ").toLowerCase();
        const b = (rowB.getValue<string[]>(columnId) ?? []).join(" ").toLowerCase();
        return a.localeCompare(b);
      },
      filterFn: (row, columnId, filterValue) => {
        const tags = row.getValue<string[]>(columnId) ?? [];
        const query = String(filterValue ?? "").trim().toLowerCase();
        if (!query) {
          return true;
        }
        return tags.some((tag) => tag.toLowerCase().includes(query));
      },
      cell: (info) =>
        renderComponent(TagsCell, {
          cidr: info.row.original.cidr,
          tags: info.getValue(),
          draft,
          locked,
          onAdd: onAddTag,
          onRemove: onRemoveTag,
        }),
    }),
  ]);

  function numericFilter(
    row: { getValue: <T>(columnId: string) => T },
    columnId: string,
    filterValue: unknown,
  ) {
    const query = String(filterValue ?? "").trim();
    if (!query) {
      return true;
    }
    const value = row.getValue<number | null>(columnId);
    if (value === null || value === undefined) {
      return false;
    }
    return String(value).includes(query);
  }

  const table = createTable({
    features,
    columns,
    getRowId: (row) => row.cidr,
    get data() {
      return rows;
    },
    enableMultiSort: false,
    autoResetPageIndex: false,
    initialState: {
      pagination: {
        pageIndex: 0,
        pageSize: PAGE_SIZE,
      },
    },
  });

  const visibleRows = $derived(table.getRowModel().rows);
  const headerGroups = $derived(table.getHeaderGroups());
  const activeFilters = $derived(
    table.getAllLeafColumns().filter((column) => column.getIsFiltered()).length,
  );
  const filteredCount = $derived(table.getPrePaginatedRowModel().rows.length);
  const orderedRows = $derived(table.getPrePaginatedRowModel().rows);
  const pagination = $derived(table.atoms.pagination.get());
  const fillerCount = $derived(Math.max(0, PAGE_SIZE - visibleRows.length));
  const rangeStart = $derived(filteredCount === 0 ? 0 : pagination.pageIndex * PAGE_SIZE + 1);
  const rangeEnd = $derived(
    filteredCount === 0 ? 0 : Math.min(filteredCount, (pagination.pageIndex + 1) * PAGE_SIZE),
  );
  const fillers = $derived(Array.from({ length: fillerCount }, (_, index) => index));
  const rangeLabel = $derived(
    filteredCount === 0
      ? "0 of 0"
      : `${rangeStart}–${rangeEnd} of ${filteredCount.toLocaleString()}`,
  );
  const canPrevious = $derived(table.getCanPreviousPage());
  const canNext = $derived(table.getCanNextPage());

  $effect(() => {
    onPageChange({
      label: rangeLabel,
      canPrevious,
      canNext,
      previous: () => table.previousPage(),
      next: () => table.nextPage(),
    });
  });

  $effect(() => {
    onFilterCount(activeFilters);
  });

  $effect(() => {
    void pageReset;
    untrack(() => table.setPageIndex(0));
  });

  $effect(() => {
    if (selectedCidrs.size === 0) {
      rangeAnchor = null;
    }
  });

  $effect(() => {
    const pageCount = Math.max(1, table.getPageCount());
    if (pagination.pageIndex >= pageCount) {
      table.setPageIndex(pageCount - 1);
    }
  });

  $effect(() => {
    if (!menu) {
      return;
    }
    function dismiss() {
      menu = null;
    }
    window.addEventListener("click", dismiss);
    return () => {
      window.removeEventListener("click", dismiss);
    };
  });

  $effect(() => {
    function selectByKeyboard(event: KeyboardEvent) {
      if (menu || isInteractive(event.target) || isMenu(event.target)) {
        return;
      }
      if (event.key === "Escape" && selectedCidrs.size > 0) {
        onSelectionChange(new Set());
        rangeAnchor = null;
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        onSelectionChange(new Set(orderedRows.map((row) => row.original.cidr)));
      }
    }
    window.addEventListener("keydown", selectByKeyboard);
    return () => window.removeEventListener("keydown", selectByKeyboard);
  });

  let menu = $state<{ cidr: string; x: number; y: number; trigger: HTMLElement | null } | null>(null);
  let rangeAnchor = $state<string | null>(null);

  function openMenu(event: MouseEvent, cidr: string) {
    event.preventDefault();
    menu = { cidr, x: event.clientX, y: event.clientY, trigger: event.currentTarget as HTMLElement };
  }

  function selectRowByKeyboard(event: KeyboardEvent, cidr: string) {
    if (isInteractive(event.target)) {
      return;
    }
    if (event.key !== "Enter" && event.key !== " ") {
      if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
        event.preventDefault();
        const trigger = event.currentTarget as HTMLElement;
        const bounds = trigger.getBoundingClientRect();
        menu = { cidr, x: bounds.left + Math.min(24, bounds.width / 2), y: bounds.top + Math.min(24, bounds.height / 2), trigger };
      }
      return;
    }
    event.preventDefault();
    const modifier = event.metaKey || event.ctrlKey;
    if (event.shiftKey && rangeAnchor) {
      const start = orderedRows.findIndex((row) => row.original.cidr === rangeAnchor);
      const end = orderedRows.findIndex((row) => row.original.cidr === cidr);
      if (start >= 0 && end >= 0) {
        const range = orderedRows
          .slice(Math.min(start, end), Math.max(start, end) + 1)
          .map((row) => row.original.cidr);
        onSelectionChange(modifier ? new Set([...selectedCidrs, ...range]) : new Set(range));
        return;
      }
    }
    const next = new Set(selectedCidrs);
    if (modifier || selectedCidrs.has(cidr)) {
      if (next.has(cidr)) {
        next.delete(cidr);
      } else {
        next.add(cidr);
      }
    } else {
      next.clear();
      next.add(cidr);
    }
    onSelectionChange(next);
    rangeAnchor = cidr;
  }

  function selectRow(event: MouseEvent, cidr: string) {
    if (isInteractive(event.target)) {
      return;
    }
    const additive = event.metaKey || event.ctrlKey;
    if (event.shiftKey && rangeAnchor) {
      const start = orderedRows.findIndex((row) => row.original.cidr === rangeAnchor);
      const end = orderedRows.findIndex((row) => row.original.cidr === cidr);
      if (start >= 0 && end >= 0) {
        const range = orderedRows
          .slice(Math.min(start, end), Math.max(start, end) + 1)
          .map((row) => row.original.cidr);
        onSelectionChange(additive ? new Set([...selectedCidrs, ...range]) : new Set(range));
        return;
      }
    }
    if (additive) {
      const next = new Set(selectedCidrs);
      if (next.has(cidr)) {
        next.delete(cidr);
      } else {
        next.add(cidr);
      }
      onSelectionChange(next);
    } else {
      onSelectionChange(new Set([cidr]));
    }
    rangeAnchor = cidr;
  }

  function isInteractive(target: EventTarget | null): boolean {
    return target instanceof Element && Boolean(target.closest("button, input, select, textarea, a"));
  }

  function isMenu(target: EventTarget | null): boolean {
    return target instanceof Element && Boolean(target.closest('[role="menu"]'));
  }

  function menuScope(cidr: string): string[] {
    return selectedCidrs.has(cidr) ? [...selectedCidrs] : [cidr];
  }

  const numericIds = new Set([
    "quantity",
    "ok",
    "connectP50",
    "connectP95",
    "ttfbP50",
    "ttfbP95",
  ]);

  const timingKeys: TimingKey[] = ["connectP50", "connectP95", "ttfbP50", "ttfbP95"];
  const groups: Record<string, string> = {
    connectP50: "Connect",
    connectP95: "Connect",
    ttfbP50: "TTFB",
    ttfbP95: "TTFB",
  };

  function cellTone(row: SubnetRow, columnId: string): string {
    if (columnId === "tags") {
      return "text-muted";
    }
    if (empty(valueOf(row, columnId))) {
      return "text-faint";
    }
    if (columnId === "ok") {
      return toneClass(okTone(row.ok, row.quantity));
    }
    if (timingKeys.includes(columnId as TimingKey)) {
      const key = columnId as TimingKey;
      return toneClass(timingTone(row[key]));
    }
    if (columnId === "cidr") {
      return "text-text";
    }
    return "text-muted";
  }

  function valueOf(row: SubnetRow, columnId: string): string | number | null {
    if (columnId === "country") {
      return row.country;
    }
    if (columnId === "cidr") {
      return row.cidr;
    }
    if (columnId === "quantity") {
      return row.quantity;
    }
    if (columnId === "ok") {
      return row.ok;
    }
    if (columnId === "lastRunAt") {
      return row.lastRunAt;
    }
    if (timingKeys.includes(columnId as TimingKey)) {
      return row[columnId as TimingKey];
    }
    return null;
  }
</script>

<div class="relative min-h-0 flex-1 overflow-hidden">
  <table class="w-full table-fixed border-collapse text-left text-sm">
    <colgroup>
      <col class="w-[168px]" />
      <col class="w-[88px]" />
      <col class="w-[72px]" />
      <col class="w-[72px]" />
      <col class="w-[128px]" />
      <col class="w-[84px]" />
      <col class="w-[84px]" />
      <col class="w-[84px]" />
      <col class="w-[84px]" />
      <col class="w-[256px]" />
    </colgroup>
    <thead class="text-xs">
      {#each headerGroups as headerGroup (headerGroup.id)}
        <tr class="h-16 border-b border-line">
          {#each headerGroup.headers as header (header.id)}
            <th
              class="h-16 overflow-hidden px-3 align-middle font-medium text-muted {header.column
                .id === 'cidr'
                ? 'pl-5'
                : ''} {header.column.id === 'tags' ? 'pr-5' : ''}"
            >
              {#if !header.isPlaceholder}
                <div class="flex h-16 flex-col {filtering ? '' : 'justify-center'}">
                  <div class="flex {filtering ? 'h-8' : 'h-full'} items-center">
                    <button
                      type="button"
                      class="inline-flex min-w-0 items-center gap-1 {header.column.getCanSort()
                        ? 'cursor-pointer select-none hover:text-text'
                        : ''} {numericIds.has(header.column.id) ? 'w-full justify-end' : ''}"
                      disabled={!header.column.getCanSort()}
                      onclick={(event) => {
                        const wasSorted = header.column.getIsSorted();
                        header.column.getToggleSortingHandler()?.(event);
                        if (header.column.getIsSorted() !== wasSorted) {
                          table.setPageIndex(0);
                        }
                      }}
                    >
                      <span
                        class="flex min-w-0 flex-col {numericIds.has(header.column.id)
                          ? 'items-end'
                          : 'items-start'}"
                      >
                        {#if groups[header.column.id]}
                          <span class="text-[11px] leading-4 text-faint">{groups[header.column.id]}</span>
                        {/if}
                        <span class="leading-4"><FlexRender {header} /></span>
                      </span>
                      <span
                        class="inline-block w-3 text-accent {header.column.getIsSorted()
                          ? ''
                          : 'opacity-0'}"
                        aria-hidden="true"
                      >
                        {header.column.getIsSorted() === "desc" ? "↓" : "↑"}
                      </span>
                    </button>
                  </div>
                  {#if filtering}
                    <div class="flex h-8 items-center">
                      {#if header.column.getCanFilter()}
                        <input
                          class="block h-6 w-full rounded-[var(--radius-control)] border border-transparent bg-raised px-1 text-xs font-normal text-text focus:border-line focus:outline-none focus:ring-1 focus:ring-accent {numericIds.has(
                            header.column.id,
                          )
                            ? 'text-right'
                            : ''}"
                          value={String(header.column.getFilterValue() ?? "")}
                          aria-label={`Filter ${String(header.column.columnDef.header)}`}
                          oninput={(event) => {
                            header.column.setFilterValue(event.currentTarget.value);
                            table.setPageIndex(0);
                          }}
                        />
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}
            </th>
          {/each}
        </tr>
      {/each}
    </thead>
    <tbody class="select-none">
      {#each visibleRows as row (row.id)}
        <tr
          class="group h-8 border-t border-line {selectedCidrs.has(row.original.cidr)
            ? 'bg-accent/10 shadow-[inset_3px_0_0_var(--color-accent)] hover:bg-accent/15'
          : 'hover:bg-raised/80'} focus-visible:relative focus-visible:z-10 focus-visible:outline-none focus-visible:shadow-[inset_0_0_0_1px_var(--color-accent)]"
          tabindex="0"
          aria-selected={selectedCidrs.has(row.original.cidr)}
          onclick={(event) => selectRow(event, row.original.cidr)}
          oncontextmenu={(event) => openMenu(event, row.original.cidr)}
          onkeydown={(event) => selectRowByKeyboard(event, row.original.cidr)}
        >
          {#each row.getAllCells() as cell (cell.id)}
            <td
              class="h-8 overflow-hidden px-3 align-middle {cell.column.id === 'cidr'
                ? 'pl-5 whitespace-nowrap'
                : ''} {cell.column.id === 'tags' ? 'pr-5' : ''} {numericIds.has(cell.column.id)
                ? 'text-right tabular-nums'
                : ''} {cell.column.id === 'lastRunAt'
                ? 'tabular-nums'
                : ''} {cellTone(row.original, cell.column.id)}"
              title={cell.column.id === "lastRunAt"
                ? lastProbeTitle(row.original.lastRunAt)
                : undefined}
            >
              <FlexRender {cell} />
            </td>
          {/each}
        </tr>
      {/each}
      {#each fillers as index (index)}
        <tr aria-hidden="true" class="h-8 border-t border-line">
          <td colspan="10" class="h-8"></td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if filteredCount === 0}
    <p
      class="pointer-events-none absolute inset-x-0 top-16 flex h-[480px] items-center justify-center text-sm text-faint"
    >
      No subnets match the filter.
    </p>
  {/if}
</div>
{#if menu}
  <RowMenu
    x={menu.x}
    y={menu.y}
    trigger={menu.trigger}
    {locked}
    onProbe={() => {
      const cidr = menu?.cidr;
      menu = null;
      if (cidr) {
        onProbeSubnet(menuScope(cidr));
      }
    }}
    onExport={() => {
      const cidr = menu?.cidr;
      menu = null;
      if (cidr) {
        onExportSubnet(menuScope(cidr));
      }
    }}
    onExportAycd={() => {
      const cidr = menu?.cidr;
      menu = null;
      if (cidr) {
        onExportAycdSubnet(menuScope(cidr));
      }
    }}
    onRemove={() => {
      const cidr = menu?.cidr;
      menu = null;
      if (cidr) {
        onRemoveSubnet(cidr);
      }
    }}
    onDismiss={() => (menu = null)}
  />
{/if}
