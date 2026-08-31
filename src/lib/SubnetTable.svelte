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
    draft,
    locked,
    filtering,
    pageReset,
    onFilterCount,
    onPageChange,
    onAddTag,
    onRemoveTag,
    onProbeSubnet,
    onExportSubnet,
    onRemoveSubnet,
  }: {
    rows: SubnetRow[];
    draft: Record<string, string>;
    locked: boolean;
    filtering: boolean;
    pageReset: number;
    onFilterCount: (count: number) => void;
    onPageChange: (page: {
      label: string;
      canPrevious: boolean;
      canNext: boolean;
      previous: () => void;
      next: () => void;
    }) => void;
    onAddTag: (cidr: string) => void;
    onRemoveTag: (cidr: string, tag: string) => void;
    onProbeSubnet: (cidr: string) => void;
    onExportSubnet: (cidr: string) => void;
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

  let menu = $state<{ cidr: string; x: number; y: number } | null>(null);

  function openMenu(event: MouseEvent, cidr: string) {
    event.preventDefault();
    menu = { cidr, x: event.clientX, y: event.clientY };
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
                          class="block h-6 w-full rounded-md bg-raised px-1 text-xs font-normal text-text focus:outline-none focus:ring-1 focus:ring-accent {numericIds.has(
                            header.column.id,
                          )
                            ? 'text-right'
                            : ''}"
                          value={String(header.column.getFilterValue() ?? "")}
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
    <tbody>
      {#each visibleRows as row (row.id)}
        <tr
          class="group h-8 border-t border-line hover:bg-raised"
          oncontextmenu={(event) => openMenu(event, row.original.cidr)}
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
    {locked}
    onProbe={() => {
      const cidr = menu?.cidr;
      menu = null;
      if (cidr) {
        onProbeSubnet(cidr);
      }
    }}
    onExport={() => {
      const cidr = menu?.cidr;
      menu = null;
      if (cidr) {
        onExportSubnet(cidr);
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
