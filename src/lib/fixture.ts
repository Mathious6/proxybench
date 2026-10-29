import {
  REPOSITORY,
  type Backend,
  type Health,
  type HealthStatus,
  type Probe,
  type RunKind,
  type RunProgress,
  type RunSummary,
  type Limits,
  type NumericSetting,
  type Settings,
  type SubnetRow,
} from "./ipc";

type Run = { kind: RunKind; stopped: boolean; finished: Promise<RunSummary> };

const DEFAULTS: Settings = {
  target: "https://cp.cloudflare.com/generate_204",
  inFlight: 128,
  startsPerSecond: 20,
  timeoutSeconds: 5,
  autoCheck: true,
  checkSample: 2,
};

const LIMITS: Limits = {
  inFlight: { min: 8, max: 1024 },
  startsPerSecond: { min: 1, max: 500 },
  timeoutSeconds: { min: 1, max: 30 },
  checkSample: { min: 1, max: 10 },
};

const LABELS: Record<NumericSetting, string> = {
  inFlight: "In flight",
  startsPerSecond: "Starts per second",
  timeoutSeconds: "Timeout seconds",
  checkSample: "Check sample",
};

function validateSettings(next: Settings): string | null {
  for (const key of Object.keys(LIMITS) as NumericSetting[]) {
    const { min, max } = LIMITS[key];
    if (!Number.isInteger(next[key]) || next[key] < min || next[key] > max) {
      return `${LABELS[key]} must be between ${min} and ${max}.`;
    }
  }
  try {
    const url = new URL(next.target.trim());
    if (url.protocol !== "https:" || !url.hostname) {
      return "Enter an HTTPS URL.";
    }
    if (url.username || url.password) {
      return "The target URL must not include credentials.";
    }
  } catch {
    return "Enter an HTTPS URL.";
  }
  return null;
}

const PROBED_AT = Date.UTC(2026, 9, 28, 14, 30);
const COUNTRIES = ["US", "GB", "DE", "NL", "FR", "CA", "JP", "SG", null];
const TAGS = [["residential"], ["mobile", "eu"], [], ["checkout"], [], ["backup"], ["stable", "us"], [], ["retail"]];

function cidrAt(index: number): string {
  if (index < 30) {
    return `198.18.${index * 7}.0/24`;
  }
  if (index < 33) {
    return `198.19.${(index - 30) * 11}.0/24`;
  }
  return ["192.0.2.0/24", "198.51.100.0/24", "203.0.113.0/24"][index - 33];
}

function quality(cidr: string): number {
  const seed = cidr.split("").reduce((sum, char) => (sum * 31 + char.charCodeAt(0)) % 9973, 7);
  const band = seed % 10;
  if (band < 6) {
    return 0.86 + (seed % 14) / 100;
  }
  if (band < 8) {
    return 0.35 + (seed % 40) / 100;
  }
  return 0;
}

function latency(cidr: string): number {
  const seed = cidr.split("").reduce((sum, char) => (sum * 37 + char.charCodeAt(0)) % 7919, 3);
  return [140, 210, 320, 460, 720, 1680][seed % 6] + (seed % 50);
}

function status(ok: number, tested: number): HealthStatus {
  if (ok === 0) {
    return "dead";
  }
  return ok / tested >= 0.8 ? "alive" : "partial";
}

function measure(row: SubnetRow, at: number): Probe {
  const tested = row.quantity;
  const ok = Math.round(tested * quality(row.cidr));
  const base = latency(row.cidr);
  const alive = ok > 0;
  return {
    at,
    ok,
    tested,
    connectP50: alive ? base : null,
    connectP95: alive ? Math.round(base * 1.6) : null,
    ttfbP50: alive ? base + 160 : null,
    ttfbP95: alive ? Math.round((base + 160) * 1.7) : null,
  };
}

function sampleHealth(row: SubnetRow, sample: number, at: number): Health {
  const tested = Math.min(sample, row.quantity);
  const ok = Math.round(tested * quality(row.cidr));
  return { status: status(ok, tested), ok, tested, at };
}

function seedRows(): SubnetRow[] {
  return Array.from({ length: 36 }, (_, index) => {
    const cidr = cidrAt(index);
    const row: SubnetRow = {
      cidr,
      country: COUNTRIES[index % COUNTRIES.length],
      quantity: 12 + ((index * 37) % 240),
      tags: TAGS[index % TAGS.length],
      health: null,
      probe: null,
    };
    if (index % 4 !== 3) {
      row.probe = measure(row, PROBED_AT - index * 3_600_000);
      row.health = { status: status(row.probe.ok, row.probe.tested), ok: row.probe.ok, tested: row.probe.tested, at: row.probe.at };
    }
    return row;
  });
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function copy<T>(value: T): T {
  return structuredClone(value);
}

export function fixtureBackend(): Backend {
  const params = new URLSearchParams(window.location.search);
  let rows: SubnetRow[] = params.get("empty") === "1" ? [] : seedRows();
  let settings: Settings = { ...DEFAULTS };
  let imports = 0;
  let active: Run | null = null;
  const listeners = new Set<(progress: RunProgress) => void>();

  function scope(cidrs: string[] | null): SubnetRow[] {
    return cidrs ? rows.filter((row) => cidrs.includes(row.cidr)) : rows;
  }

  function replace(next: SubnetRow) {
    rows = rows.map((row) => (row.cidr === next.cidr ? next : row));
  }

  function emit(progress: RunProgress) {
    for (const listener of listeners) {
      listener(copy(progress));
    }
  }

  function start(kind: RunKind, work: (run: Run) => Promise<SubnetRow[]>): Promise<RunSummary> {
    const run: Run = { kind, stopped: false, finished: Promise.resolve({ kind, stopped: false, rows: [] }) };
    run.finished = work(run)
      .then((done) => ({ kind, stopped: run.stopped, rows: copy(done) }))
      .finally(() => {
        if (active === run) {
          active = null;
        }
      });
    active = run;
    return run.finished;
  }

  async function check(cidrs: string[] | null): Promise<RunSummary> {
    if (active) {
      throw "A run is already in progress.";
    }
    const targets = scope(cidrs).map((row) => row.cidr);
    return start("check", async (run) => {
      const done: SubnetRow[] = [];
      const begun = Date.now();
      for (const cidr of targets) {
        await sleep(90);
        if (run.stopped) {
          break;
        }
        const row = rows.find((candidate) => candidate.cidr === cidr);
        if (!row) {
          continue;
        }
        const next = { ...row, health: sampleHealth(row, settings.checkSample, Date.now()) };
        replace(next);
        done.push(next);
        const rate = done.length / ((Date.now() - begun) / 1000);
        emit({ kind: "check", done: done.length, total: targets.length, etaSeconds: Math.round((targets.length - done.length) / rate), row: next });
      }
      return done;
    });
  }

  async function probe(cidrs: string[] | null): Promise<RunSummary> {
    if (active?.kind === "probe") {
      throw "A probe is already running.";
    }
    if (active) {
      active.stopped = true;
      await active.finished;
    }
    const targets = scope(cidrs).map((row) => row.cidr);
    const total = scope(cidrs).reduce((sum, row) => sum + row.quantity, 0);
    return start("probe", async (run) => {
      const done: SubnetRow[] = [];
      let proxies = 0;
      const begun = Date.now();
      for (const cidr of targets) {
        const row = rows.find((candidate) => candidate.cidr === cidr);
        if (!row) {
          continue;
        }
        await sleep(40 + row.quantity);
        if (run.stopped) {
          break;
        }
        const measured = measure(row, Date.now());
        const next: SubnetRow = {
          ...row,
          country: row.country ?? "US",
          probe: measured,
          health: { status: status(measured.ok, measured.tested), ok: measured.ok, tested: measured.tested, at: measured.at },
        };
        replace(next);
        done.push(next);
        proxies += row.quantity;
        const rate = proxies / ((Date.now() - begun) / 1000);
        emit({ kind: "probe", done: proxies, total, etaSeconds: Math.round((total - proxies) / rate), row: next });
      }
      return done;
    });
  }

  return {
    rows: async () => copy(rows),
    importPaths: async (paths) => {
      await sleep(200);
      if (active?.kind === "probe") {
        throw "Wait for the probe to finish.";
      }
      imports += 1;
      const added = [`198.19.${100 + imports * 2}.0/24`, `198.19.${101 + imports * 2}.0/24`];
      const grown = rows.length > 0 ? [rows[imports % rows.length].cidr] : [];
      rows = rows.map((row) => (grown.includes(row.cidr) ? { ...row, quantity: row.quantity + 10, health: null, probe: null } : row));
      rows = [...rows, ...added.map((cidr) => ({ cidr, country: null, quantity: 24 + paths.length, tags: [], health: null, probe: null }))];
      return { rows: copy(rows), added, grown, skipped: 3 };
    },
    setTags: async (cidr, tags) => {
      const clean: string[] = [];
      for (const tag of tags.map((value) => value.trim()).filter(Boolean)) {
        if (!clean.some((kept) => kept.toLowerCase() === tag.toLowerCase())) {
          clean.push(tag);
        }
      }
      rows = rows.map((row) => (row.cidr === cidr ? { ...row, tags: clean } : row));
      return clean;
    },
    remove: async (cidrs) => {
      if (active) {
        throw "Stop the run before removing subnets.";
      }
      rows = rows.filter((row) => !cidrs.includes(row.cidr));
      return copy(rows);
    },
    settings: async () => copy({ current: settings, defaults: DEFAULTS, limits: LIMITS }),
    saveSettings: async (next) => {
      await sleep(120);
      const error = validateSettings(next);
      if (error) {
        throw error;
      }
      settings = { ...next, target: next.target.trim() };
      return { ...settings };
    },
    check,
    probe,
    stop: async () => {
      if (active) {
        active.stopped = true;
      }
    },
    exportTxt: async (_path, cidrs) => {
      await sleep(150);
      return scope(cidrs).length > 0 ? 1 : 0;
    },
    exportAycd: async (_path, cidrs) => {
      await sleep(150);
      return scope(cidrs).reduce((sum, row) => sum + row.quantity, 0);
    },
    onProgress: async (handler) => {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
    onDragDrop: async (handler) => {
      const over = (event: DragEvent) => {
        event.preventDefault();
        handler({ kind: "over" });
      };
      const leave = (event: DragEvent) => {
        if (!event.relatedTarget) {
          handler({ kind: "leave" });
        }
      };
      const drop = (event: DragEvent) => {
        event.preventDefault();
        handler({ kind: "drop", paths: [...(event.dataTransfer?.files ?? [])].map((file) => file.name) });
      };
      window.addEventListener("dragover", over);
      window.addEventListener("dragleave", leave);
      window.addEventListener("drop", drop);
      return () => {
        window.removeEventListener("dragover", over);
        window.removeEventListener("dragleave", leave);
        window.removeEventListener("drop", drop);
      };
    },
    pickFiles: async () => ["proxies.txt"],
    pickFolder: async () => "exports",
    pickAycdFile: async () => "proxybench-aycd.json",
    version: async () => __APP_VERSION__,
    openRepository: async () => {
      window.open(REPOSITORY, "_blank", "noopener");
    },
    findUpdate: async () => {
      await sleep(400);
      return null;
    },
  };
}
