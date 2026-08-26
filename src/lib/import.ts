export type SubnetRow = {
  cidr: string;
  country: string | null;
  quantity: number;
  tags: string[];
  ok: number | null;
  connectP50: number | null;
  connectP95: number | null;
  ttfbP50: number | null;
  ttfbP95: number | null;
  lastRunAt: number | null;
};

export type ImportResult = {
  rows: SubnetRow[];
  skipped: number;
  grown: string[];
};

export type Metrics = {
  cidr: string;
  tested: number;
  ok: number;
  connectP50: number | null;
  connectP95: number | null;
  ttfbP50: number | null;
  ttfbP95: number | null;
};

export type Progress = {
  done: number;
  total: number;
  etaSeconds: number | null;
  metrics: Metrics;
};

export type RunResult = {
  completedAt: number;
  metrics: Metrics[];
};

export function emptyMetrics(): Pick<
  SubnetRow,
  "ok" | "connectP50" | "connectP95" | "ttfbP50" | "ttfbP95"
> {
  return {
    ok: null,
    connectP50: null,
    connectP95: null,
    ttfbP50: null,
    ttfbP95: null,
  };
}

export function withMetrics(row: SubnetRow, metrics: Metrics, at?: number): SubnetRow {
  return {
    ...row,
    ok: metrics.ok,
    connectP50: metrics.connectP50,
    connectP95: metrics.connectP95,
    ttfbP50: metrics.ttfbP50,
    ttfbP95: metrics.ttfbP95,
    lastRunAt: at ?? row.lastRunAt,
  };
}
