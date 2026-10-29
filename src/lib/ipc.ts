import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check as checkUpdate } from "@tauri-apps/plugin-updater";
import { fixtureBackend } from "./fixture";

export type HealthStatus = "alive" | "partial" | "dead";

export type Health = {
  status: HealthStatus;
  ok: number;
  tested: number;
  at: number;
};

export type Probe = {
  at: number;
  ok: number;
  tested: number;
  connectP50: number | null;
  connectP95: number | null;
  ttfbP50: number | null;
  ttfbP95: number | null;
};

export type SubnetRow = {
  cidr: string;
  country: string | null;
  quantity: number;
  tags: string[];
  health: Health | null;
  probe: Probe | null;
};

export type Settings = {
  target: string;
  inFlight: number;
  startsPerSecond: number;
  timeoutSeconds: number;
  autoCheck: boolean;
  checkSample: number;
};

export type NumericSetting = "inFlight" | "startsPerSecond" | "timeoutSeconds" | "checkSample";

export type Bounds = { min: number; max: number };

export type Limits = Record<NumericSetting, Bounds>;

export type SettingsSheet = {
  current: Settings;
  defaults: Settings;
  limits: Limits;
};

export type ImportReport = {
  rows: SubnetRow[];
  added: string[];
  grown: string[];
  skipped: number;
};

export type RunKind = "check" | "probe";

export type RunProgress = {
  kind: RunKind;
  done: number;
  total: number;
  etaSeconds: number | null;
  row: SubnetRow;
};

export type RunSummary = {
  kind: RunKind;
  stopped: boolean;
  rows: SubnetRow[];
};

export type DragDrop = { kind: "over" } | { kind: "leave" } | { kind: "drop"; paths: string[] };

export type AvailableUpdate = {
  version: string;
  install: (onProgress: (fraction: number | null) => void) => Promise<void>;
};

export type Backend = {
  rows: () => Promise<SubnetRow[]>;
  importPaths: (paths: string[]) => Promise<ImportReport>;
  setTags: (cidr: string, tags: string[]) => Promise<string[]>;
  remove: (cidrs: string[]) => Promise<SubnetRow[]>;
  settings: () => Promise<SettingsSheet>;
  saveSettings: (settings: Settings) => Promise<Settings>;
  check: (cidrs: string[] | null) => Promise<RunSummary>;
  probe: (cidrs: string[] | null) => Promise<RunSummary>;
  stop: () => Promise<void>;
  exportTxt: (path: string, cidrs: string[] | null) => Promise<number>;
  exportAycd: (path: string, cidrs: string[] | null) => Promise<number>;
  onProgress: (handler: (progress: RunProgress) => void) => Promise<() => void>;
  onDragDrop: (handler: (event: DragDrop) => void) => Promise<() => void>;
  pickFiles: () => Promise<string[] | null>;
  pickFolder: () => Promise<string | null>;
  pickAycdFile: () => Promise<string | null>;
  version: () => Promise<string>;
  openRepository: () => Promise<void>;
  findUpdate: () => Promise<AvailableUpdate | null>;
};

export const REPOSITORY = "https://github.com/Mathious6/proxybench";

const tauriBackend: Backend = {
  rows: () => invoke("rows"),
  importPaths: (paths) => invoke("import_paths", { paths }),
  setTags: (cidr, tags) => invoke("set_tags", { cidr, tags }),
  remove: (cidrs) => invoke("remove", { cidrs }),
  settings: () => invoke("settings"),
  saveSettings: (settings) => invoke("save_settings", { settings }),
  check: (cidrs) => invoke("check", { cidrs }),
  probe: (cidrs) => invoke("probe", { cidrs }),
  stop: () => invoke("stop"),
  exportTxt: (path, cidrs) => invoke("export_txt", { path, cidrs }),
  exportAycd: (path, cidrs) => invoke("export_aycd", { path, cidrs }),
  onProgress: (handler) => listen<RunProgress>("run-progress", (event) => handler(event.payload)),
  onDragDrop: (handler) =>
    getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "drop") {
        handler({ kind: "drop", paths: payload.paths });
      } else if (payload.type === "leave") {
        handler({ kind: "leave" });
      } else {
        handler({ kind: "over" });
      }
    }),
  pickFiles: async () => {
    const picked = await open({ multiple: true, filters: [{ name: "Proxy lists", extensions: ["txt"] }] });
    if (picked === null) {
      return null;
    }
    return Array.isArray(picked) ? picked : [picked];
  },
  pickFolder: async () => {
    const picked = await open({ directory: true });
    return Array.isArray(picked) ? (picked[0] ?? null) : picked;
  },
  pickAycdFile: () =>
    save({
      title: "Export AYCD JSON",
      defaultPath: "proxybench-aycd.json",
      filters: [{ name: "AYCD JSON", extensions: ["json"] }],
    }),
  version: () => getVersion(),
  openRepository: () => openUrl(REPOSITORY),
  findUpdate: async () => {
    const update = await checkUpdate({ timeout: 30_000 });
    if (!update) {
      return null;
    }
    return {
      version: update.version,
      install: async (onProgress) => {
        let downloaded = 0;
        let total: number | null = null;
        await update.downloadAndInstall(
          (event) => {
            if (event.event === "Started") {
              total = event.data.contentLength ?? null;
            } else if (event.event === "Progress") {
              downloaded += event.data.chunkLength;
            } else {
              downloaded = total ?? downloaded;
            }
            onProgress(total ? Math.min(1, downloaded / total) : null);
          },
          { timeout: 10 * 60 * 1000 },
        );
        await relaunch();
      },
    };
  },
};

export const ipc: Backend = "__TAURI_INTERNALS__" in window ? tauriBackend : fixtureBackend();
