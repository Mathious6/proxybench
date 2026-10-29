import type { HealthStatus, SubnetRow } from "./ipc";

export type HealthKey = HealthStatus | "unknown";

export type SortKey =
  | "health"
  | "cidr"
  | "country"
  | "quantity"
  | "ok"
  | "connectP50"
  | "connectP95"
  | "ttfbP50"
  | "ttfbP95"
  | "lastProbe";

export type SortDirection = "asc" | "desc";

export type Sort = { key: SortKey; direction: SortDirection };

export type Gesture = "replace" | "toggle" | "range" | "extend";

export const PAGE_SIZE = 15;

export const HEALTH_KEYS: HealthKey[] = ["alive", "partial", "dead", "unknown"];

const HEALTH_RANK: Record<HealthKey, number | null> = { dead: 0, partial: 1, alive: 2, unknown: null };

export function healthKey(row: SubnetRow): HealthKey {
  return row.health?.status ?? "unknown";
}

function address(cidr: string): number {
  return cidr
    .split("/")[0]
    .split(".")
    .reduce((sum, octet) => sum * 256 + Number(octet), 0);
}

function sortValue(row: SubnetRow, key: SortKey): number | string | null {
  switch (key) {
    case "health":
      return HEALTH_RANK[healthKey(row)];
    case "cidr":
      return address(row.cidr);
    case "country":
      return row.country;
    case "quantity":
      return row.quantity;
    case "ok":
      return row.probe?.ok ?? null;
    case "lastProbe":
      return row.probe?.at ?? null;
    default:
      return row.probe?.[key] ?? null;
  }
}

function compare(a: SubnetRow, b: SubnetRow, sort: Sort): number {
  const left = sortValue(a, sort.key);
  const right = sortValue(b, sort.key);
  if (left === null || right === null) {
    return left === right ? 0 : left === null ? 1 : -1;
  }
  const order = typeof left === "string" ? left.localeCompare(String(right)) : left - Number(right);
  return sort.direction === "asc" ? order : -order;
}

function matches(row: SubnetRow, query: string): boolean {
  if (!query) {
    return true;
  }
  return (
    row.cidr.includes(query) ||
    (row.country ?? "").toLowerCase().includes(query) ||
    row.tags.some((tag) => tag.toLowerCase().includes(query))
  );
}

export class View {
  sort = $state<Sort | null>(null);
  query = $state("");
  status = $state<HealthKey | null>(null);
  selected = $state<ReadonlySet<string>>(new Set());
  page = $state(0);
  #anchor: string | null = null;
  #rows: () => SubnetRow[];

  constructor(rows: () => SubnetRow[]) {
    this.#rows = rows;
  }

  filtered = $derived.by(() => {
    const query = this.query.trim().toLowerCase();
    return this.#rows().filter((row) => matches(row, query) && (this.status === null || healthKey(row) === this.status));
  });

  ordered = $derived.by(() => {
    const sort = this.sort;
    return sort ? [...this.filtered].sort((a, b) => compare(a, b, sort)) : this.filtered;
  });

  pageCount = $derived(Math.max(1, Math.ceil(this.ordered.length / PAGE_SIZE)));

  currentPage = $derived(Math.min(this.page, this.pageCount - 1));

  visible = $derived(this.ordered.slice(this.currentPage * PAGE_SIZE, (this.currentPage + 1) * PAGE_SIZE));

  range = $derived({
    first: this.ordered.length === 0 ? 0 : this.currentPage * PAGE_SIZE + 1,
    last: Math.min(this.ordered.length, (this.currentPage + 1) * PAGE_SIZE),
    total: this.ordered.length,
  });

  tally = $derived.by(() => {
    const counts: Record<HealthKey, number> = { alive: 0, partial: 0, dead: 0, unknown: 0 };
    for (const row of this.#rows()) {
      counts[healthKey(row)] += 1;
    }
    return counts;
  });

  scope = $derived(this.selected.size > 0 ? [...this.selected] : null);

  filtering = $derived(this.query.trim() !== "" || this.status !== null);

  toggleSort(key: SortKey) {
    const current = this.sort;
    this.sort = { key, direction: current?.key === key && current.direction === "asc" ? "desc" : "asc" };
    this.page = 0;
  }

  setQuery(query: string) {
    this.query = query;
    this.page = 0;
  }

  toggleStatus(status: HealthKey) {
    this.status = this.status === status ? null : status;
    this.page = 0;
  }

  clearFilter() {
    this.query = "";
    this.status = null;
    this.page = 0;
  }

  previousPage() {
    this.page = Math.max(0, this.currentPage - 1);
  }

  nextPage() {
    this.page = Math.min(this.pageCount - 1, this.currentPage + 1);
  }

  pick(cidr: string, gesture: Gesture) {
    const anchor = this.#anchor;
    if ((gesture === "range" || gesture === "extend") && anchor) {
      const cidrs = this.ordered.map((row) => row.cidr);
      const start = cidrs.indexOf(anchor);
      const end = cidrs.indexOf(cidr);
      if (start >= 0 && end >= 0) {
        const span = cidrs.slice(Math.min(start, end), Math.max(start, end) + 1);
        this.selected = new Set(gesture === "extend" ? [...this.selected, ...span] : span);
        return;
      }
    }
    if (gesture === "toggle" || gesture === "extend") {
      const next = new Set(this.selected);
      if (!next.delete(cidr)) {
        next.add(cidr);
      }
      this.selected = next;
    } else {
      this.selected = new Set([cidr]);
    }
    this.#anchor = cidr;
  }

  selectFiltered() {
    this.selected = new Set(this.ordered.map((row) => row.cidr));
  }

  clearSelection() {
    this.selected = new Set();
    this.#anchor = null;
  }

  retain(rows: SubnetRow[]) {
    const present = new Set(rows.map((row) => row.cidr));
    this.selected = new Set([...this.selected].filter((cidr) => present.has(cidr)));
    if (this.#anchor && !present.has(this.#anchor)) {
      this.#anchor = null;
    }
  }

  menuScope(cidr: string): string[] {
    return this.selected.has(cidr) ? [...this.selected] : [cidr];
  }
}
