<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount, tick } from "svelte";
  import type { ImportResult, Progress, RunResult, SubnetRow } from "$lib/import";
  import { emptyMetrics, withMetrics } from "$lib/import";
  import SubnetTable from "$lib/SubnetTable.svelte";
  import ExportMenu from "$lib/ExportMenu.svelte";
  import SelectionActions from "$lib/SelectionActions.svelte";
  import UpdateControl from "$lib/UpdateControl.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Dialog from "$lib/ui/Dialog.svelte";

  let rows = $state<SubnetRow[]>([]);
  let hovering = $state(false);
  let busy = $state(false);
  let running = $state(false);
  let asking = $state(false);
  let target = $state("");
  let done = $state(0);
  let total = $state(0);
  let etaSeconds = $state<number | null>(null);
  let notice = $state("");
  let failed = $state(false);
  let working = $state("");
  let draft = $state<Record<string, string>>({});
  let filtering = $state(false);
  let filterCount = $state(0);
  let selectedCidrs = $state<Set<string>>(new Set());
  let probeCidrs = $state<string[] | null>(null);
  let pageReset = $state(0);
  let version = $state("");
  let updating = $state(false);
  let exportMenuOpen = $state(false);
  let exportMenuTrigger = $state<HTMLButtonElement | null>(null);
  let paging = $state({
    label: "0 of 0",
    canPrevious: false,
    canNext: false,
    previous: () => {},
    next: () => {},
  });
  let noticeTimer: ReturnType<typeof setTimeout> | null = null;

  const screenshotMode = import.meta.env.DEV && new URLSearchParams(window.location.search).get("screenshot") === "1";

  const locked = $derived(busy || running || updating);
  const selectedScope = $derived(selectedCidrs.size > 0 ? [...selectedCidrs] : null);

  $effect(() => {
    const available = new Set(rows.map((row) => row.cidr));
    const next = new Set([...selectedCidrs].filter((cidr) => available.has(cidr)));
    if (next.size !== selectedCidrs.size) {
      selectedCidrs = next;
    }
  });

  $effect(() => {
    if (selectedCidrs.size > 0) {
      exportMenuOpen = false;
    }
  });

  function showNotice(message: string, isError: boolean) {
    if (noticeTimer) {
      clearTimeout(noticeTimer);
      noticeTimer = null;
    }
    notice = message;
    failed = isError;
    if (!isError && message) {
      noticeTimer = setTimeout(() => {
        notice = "";
        noticeTimer = null;
      }, 4000);
    }
  }

  function clearNotice() {
    if (noticeTimer) {
      clearTimeout(noticeTimer);
      noticeTimer = null;
    }
    notice = "";
    failed = false;
  }

  onMount(() => {
    if (screenshotMode) {
      void loadScreenshot();
      return;
    }
    let disposed = false;
    let stopDrop = () => {};
    let stopProgress = () => {};
    void getCurrentWebview()
      .onDragDropEvent(async (event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          hovering = true;
          return;
        }
        if (event.payload.type === "leave") {
          hovering = false;
          return;
        }
        hovering = false;
        await importPaths(event.payload.paths);
      })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        stopDrop = unlisten;
      })
      .catch((error) => {
        showNotice(String(error), true);
      });
    void listen<Progress>("run-progress", (event) => {
      applyProgress(event.payload);
    })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        stopProgress = unlisten;
      })
      .catch((error) => {
        showNotice(String(error), true);
      });
    void getVersion()
      .then((value) => {
        version = value;
      })
      .catch(() => {});
    void invoke<string | null>("last_target")
      .then((saved) => {
        if (saved) {
          target = saved;
        }
      })
      .catch(() => {});
    void invoke<SubnetRow[]>("session_rows")
      .then((stored) => {
        if (stored.length === 0) {
          return;
        }
        rows = stored;
        draft = Object.fromEntries(stored.map((row) => [row.cidr, ""]));
      })
      .catch((error) => {
        showNotice(String(error), true);
      });
    return () => {
      disposed = true;
      stopDrop();
      stopProgress();
      if (noticeTimer) {
        clearTimeout(noticeTimer);
        noticeTimer = null;
      }
    };
  });

  async function loadScreenshot() {
    const { screenshotRows, screenshotSelectedCidrs } = await import("$lib/screenshot");
    rows = screenshotRows;
    draft = Object.fromEntries(screenshotRows.map((row) => [row.cidr, ""]));
    selectedCidrs = new Set(screenshotSelectedCidrs);
    version = __SCREENSHOT_VERSION__;
    await tick();
    document.documentElement.dataset.screenshotReady = "true";
  }

  function applyProgress(progress: Progress) {
    done = progress.done;
    total = progress.total;
    etaSeconds = progress.etaSeconds;
    rows = rows.map((row) =>
      row.cidr === progress.metrics.cidr ? withMetrics(row, progress.metrics) : row,
    );
  }

  async function pickFiles() {
    let selected;
    try {
      selected = await open({
        multiple: true,
        filters: [{ name: "Proxy lists", extensions: ["txt"] }],
      });
    } catch (error) {
      showNotice(String(error), true);
      return;
    }
    if (selected === null) {
      return;
    }
    await importPaths(Array.isArray(selected) ? selected : [selected]);
  }

  async function exportFiles(cidrs: string[] | null = selectedScope) {
    exportMenuOpen = false;
    if (locked || rows.length === 0) {
      return;
    }
    let selected;
    try {
      selected = await open({
        directory: true,
      });
    } catch (error) {
      showNotice(String(error), true);
      return;
    }
    if (selected === null) {
      return;
    }
    const path = Array.isArray(selected) ? selected[0] : selected;
    busy = true;
    working = "Exporting…";
    clearNotice();
    try {
      const written = await invoke<number>("export_dir", { path, cidrs });
      showNotice(written === 1 ? "Exported 1 file" : `Exported ${written} files`, false);
    } catch (error) {
      showNotice(String(error), true);
    } finally {
      busy = false;
      working = "";
    }
  }

  async function exportAycd(cidrs: string[] | null = selectedScope) {
    if (locked || rows.length === 0) {
      return;
    }
    exportMenuOpen = false;
    let path;
    try {
      path = await save({
        defaultPath: "proxybench-aycd.json",
        filters: [{ name: "AYCD JSON", extensions: ["json"] }],
        title: "Export for AYCD",
      });
    } catch (error) {
      showNotice(String(error), true);
      return;
    }
    if (path === null) {
      return;
    }
    busy = true;
    working = "Exporting…";
    clearNotice();
    try {
      const written = await invoke<number>("export_aycd", { path, cidrs });
      showNotice(written === 1 ? "Exported 1 proxy for AYCD" : `Exported ${written} proxies for AYCD`, false);
    } catch (error) {
      showNotice(String(error), true);
    } finally {
      busy = false;
      working = "";
    }
  }

  function closeExportMenu() {
    exportMenuOpen = false;
  }

  $effect(() => {
    if (!exportMenuOpen) {
      return;
    }
    function dismiss(event: MouseEvent) {
      if (!(event.target as Element).closest("[data-export-menu]")) {
        exportMenuOpen = false;
      }
    }
    window.addEventListener("click", dismiss);
    return () => window.removeEventListener("click", dismiss);
  });

  async function importPaths(paths: string[]) {
    if (locked || paths.length === 0) {
      return;
    }
    busy = true;
    working = "Importing…";
    clearNotice();
    try {
      const result = await invoke<ImportResult>("import_paths", { paths });
      rows = result.rows;
      draft = Object.fromEntries(result.rows.map((row) => [row.cidr, draft[row.cidr] ?? ""]));
      pageReset += 1;
      if (result.skipped > 0) {
        showNotice(`${result.skipped} lines ignored`, false);
      } else if (result.grown.length > 0) {
        showNotice("Added proxies to existing subnets", false);
      }
    } catch (error) {
      showNotice(String(error), true);
    } finally {
      busy = false;
      working = "";
    }
  }

  const probeScope = $derived.by(() => {
    const cidrs = probeCidrs;
    if (cidrs) {
      const scoped = rows.filter((row) => cidrs.includes(row.cidr));
      const proxies = scoped.reduce((sum, row) => sum + row.quantity, 0);
      return `${scoped.length.toLocaleString()} ${scoped.length === 1 ? "subnet" : "subnets"} · ${proxies.toLocaleString()} ${proxies === 1 ? "proxy" : "proxies"}`;
    }
    const n = rows.length;
    const proxies = rows.reduce((sum, row) => sum + row.quantity, 0);
    return `${n.toLocaleString()} ${n === 1 ? "subnet" : "subnets"} · ${proxies.toLocaleString()} ${proxies === 1 ? "proxy" : "proxies"}`;
  });

  async function openProbe() {
    if (locked || rows.length === 0) {
      return;
    }
    exportMenuOpen = false;
    probeCidrs = selectedScope;
    asking = true;
  }

  function cancelProbe() {
    asking = false;
    probeCidrs = null;
  }

  async function probeSubnets(cidrs: string[]) {
    if (locked) {
      return;
    }
    exportMenuOpen = false;
    probeCidrs = cidrs;
    if (target.trim()) {
      await confirmProbe();
      return;
    }
    asking = true;
  }

  async function confirmProbe() {
    const url = target.trim();
    if (!url || locked) {
      return;
    }
    const cidrs = probeCidrs;
    asking = false;
    running = true;
    clearNotice();
    done = 0;
    const scoped = cidrs ? rows.filter((row) => cidrs.includes(row.cidr)) : rows;
    total = scoped.reduce((sum, row) => sum + row.quantity, 0);
    etaSeconds = null;
    const previous = rows;
    rows = rows.map((row) =>
      (!cidrs || cidrs.includes(row.cidr)) && row.quantity > 0 ? { ...row, ...emptyMetrics() } : row,
    );
    try {
      const probed = await invoke<RunResult>("start_run", { url, cidrs });
      const byCidr = new Map(probed.metrics.map((metrics) => [metrics.cidr, metrics]));
      rows = rows.map((row) => {
        const metrics = byCidr.get(row.cidr);
        const country = probed.countries[row.cidr];
        const next = metrics ? withMetrics(row, metrics, probed.completedAt) : row;
        return country ? { ...next, country } : next;
      });
    } catch (error) {
      try {
        const stored = await invoke<SubnetRow[]>("session_rows");
        rows = stored;
        draft = Object.fromEntries(stored.map((row) => [row.cidr, draft[row.cidr] ?? ""]));
        showNotice(String(error), true);
      } catch (reloadError) {
        rows = previous;
        showNotice(`${String(error)} · ${String(reloadError)}`, true);
      }
    } finally {
      running = false;
      etaSeconds = null;
      probeCidrs = null;
    }
  }

  async function commitTag(cidr: string) {
    const value = (draft[cidr] ?? "").trim();
    if (!value || running) {
      return;
    }
    const next = [...(rows.find((row) => row.cidr === cidr)?.tags ?? []), value];
    await saveTags(cidr, next);
    draft[cidr] = "";
  }

  async function removeTag(cidr: string, tag: string) {
    if (running) {
      return;
    }
    const current = rows.find((row) => row.cidr === cidr)?.tags ?? [];
    await saveTags(
      cidr,
      current.filter((item) => item !== tag),
    );
  }

  async function removeSubnet(cidr: string) {
    if (locked) {
      return;
    }
    busy = true;
    working = "Working…";
    clearNotice();
    try {
      const stored = await invoke<SubnetRow[]>("remove_subnet", { cidr });
      rows = stored;
      const nextDraft = { ...draft };
      delete nextDraft[cidr];
      draft = nextDraft;
    } catch (error) {
      showNotice(String(error), true);
    } finally {
      busy = false;
      working = "";
    }
  }

  async function saveTags(cidr: string, tags: string[]) {
    try {
      const stored = await invoke<string[]>("set_tags", { cidr, tags });
      rows = rows.map((row) => (row.cidr === cidr ? { ...row, tags: stored } : row));
    } catch (error) {
      showNotice(String(error), true);
    }
  }

  function etaLabel(seconds: number | null) {
    if (seconds === null) {
      return "";
    }
    if (seconds < 60) {
      return ` · ~${seconds}s left`;
    }
    const minutes = Math.round(seconds / 60);
    return ` · ~${minutes}m left`;
  }

  async function openRepository() {
    try {
      await openUrl("https://github.com/Mathious6/proxybench");
    } catch {
      showNotice("Could not open GitHub", true);
    }
  }
</script>

<main class="relative flex h-full min-h-0 flex-col overflow-hidden bg-bg text-text">
  {#if hovering}
    <div
      class="pointer-events-none absolute inset-0 z-10 flex items-center justify-center border-2 border-dashed border-accent bg-bg/85"
    >
      <p class="text-sm text-text">Drop to import</p>
    </div>
  {/if}
  {#if asking}
    <Dialog label="Probe" onDismiss={cancelProbe}>
      <form
        onsubmit={(event) => {
          event.preventDefault();
          void confirmProbe();
        }}
      >
        <h2 class="text-sm font-medium">Probe</h2>
        {#if probeScope}
          <p class="mt-1 text-xs text-faint">{probeScope}</p>
        {/if}
        <label class="mt-4 block text-xs text-faint" for="target-url">HTTPS URL</label>
        <input
          id="target-url"
          class="mt-1 w-full rounded-[var(--radius-control)] border border-line bg-bg px-3 py-2 font-mono text-sm text-text placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
          type="url"
          placeholder="https://example.com/"
          bind:value={target}
        />
        <div class="mt-5 flex justify-end gap-2">
          <Button variant="outline" onclick={cancelProbe}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={!target.trim()}>Probe</Button>
        </div>
      </form>
    </Dialog>
  {/if}
  {#if rows.length === 0}
    <div class="flex min-h-0 flex-1 flex-col items-center justify-center px-6 text-center">
      <h1 class="text-sm font-medium">proxybench</h1>
      <p class="mt-2 text-xs text-faint">Drop a .txt file or a folder of .txt files.</p>
      <Button variant="primary" class="mt-6" disabled={locked} onclick={pickFiles}>Open files</Button>
      {#if busy}
        <p class="mt-4 text-sm text-faint">{working || "Importing…"}</p>
      {/if}
    </div>
  {:else}
    <div class="flex min-h-0 flex-1 flex-col">
      <header class="flex h-16 shrink-0 items-center justify-between gap-4 border-b border-line px-5">
        <div class="min-w-0">
          <h1 class="text-sm font-medium">proxybench</h1>
          <p class="text-xs tabular-nums text-faint">
            {#if busy && !running}
              {working || "Working…"}
            {:else}
              {rows.length} {rows.length === 1 ? "subnet" : "subnets"}
            {/if}
          </p>
        </div>
        <div class="flex items-center gap-2">
          <Button
            variant={filtering ? "primary" : "outline"}
            aria-pressed={filtering}
            onclick={() => (filtering = !filtering)}
          >
            Filter{filterCount > 0 ? ` ${filterCount}` : ""}
          </Button>
          <Button variant="outline" disabled={locked} onclick={pickFiles}>Open files</Button>
          {#if selectedCidrs.size > 0}
            <SelectionActions
              {locked}
              onExport={() => void exportFiles()}
              onExportAycd={() => void exportAycd()}
              onProbe={openProbe}
            />
          {:else}
            <div class="relative flex" data-export-menu>
              <Button variant="outline" joined="start" disabled={locked} onclick={() => void exportFiles()}>Export</Button>
              <Button
                bind:ref={exportMenuTrigger}
                variant="outline"
                size="narrowIcon"
                joined="end"
                aria-label="More export options"
                aria-expanded={exportMenuOpen}
                aria-haspopup="menu"
                disabled={locked}
                onclick={() => (exportMenuOpen = !exportMenuOpen)}
              >▾</Button>
              {#if exportMenuOpen}
                <ExportMenu {locked} trigger={exportMenuTrigger} onAycd={() => void exportAycd()} onDismiss={closeExportMenu} />
              {/if}
            </div>
            <Button variant="primary" disabled={locked} onclick={openProbe}>Probe all</Button>
          {/if}
        </div>
      </header>
      <SubnetTable
        {rows}
        {selectedCidrs}
        {draft}
        {locked}
        {filtering}
        {pageReset}
        onFilterCount={(count) => (filterCount = count)}
        onSelectionChange={(cidrs) => (selectedCidrs = cidrs)}
        onPageChange={(page) => (paging = page)}
        onAddTag={commitTag}
        onRemoveTag={removeTag}
        onProbeSubnet={probeSubnets}
        onExportSubnet={exportFiles}
        onExportAycdSubnet={exportAycd}
        onRemoveSubnet={removeSubnet}
      />
    </div>
  {/if}
  <footer class="flex h-12 shrink-0 items-center border-t border-line px-5">
    <div class="flex shrink-0 items-center gap-1.5 pr-3">
      {#if version}
        <button
          type="button"
          class="rounded-[var(--radius-control)] px-1 text-xs tabular-nums text-faint hover:text-text hover:underline hover:decoration-dotted hover:underline-offset-2 focus:outline-none focus:ring-1 focus:ring-accent"
          aria-label={`proxybench v${version} — open repository on GitHub`}
          title="https://github.com/Mathious6/proxybench"
          onclick={openRepository}
        >
          v{version}
        </button>
      {/if}
      <UpdateControl
        disabled={busy || running}
        onBusyChange={(value) => (updating = value)}
        onNotice={showNotice}
      />
    </div>
    <span class="h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>
    <div class="min-w-0 flex-1 px-3">
      {#if running && total > 0}
        <div class="flex min-w-0 items-center gap-3">
          <div class="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-raised">
            <div class="h-full bg-accent" style="width: {(done / total) * 100}%"></div>
          </div>
          <span class="shrink-0 text-xs tabular-nums text-muted">
            {done}/{total}{etaLabel(etaSeconds)}
          </span>
        </div>
      {:else if notice}
        <p
          class="flex min-w-0 items-center gap-2 truncate text-xs {failed ? 'text-bad' : 'text-faint'}"
          title={notice}
        >
          {#if failed}
            <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-bad"></span>
          {/if}
          {notice}
        </p>
      {/if}
    </div>
    {#if rows.length > 0}
      <span class="h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>
      <div class="flex shrink-0 items-center gap-2 pl-3">
        <span class="min-w-[112px] text-right text-xs tabular-nums text-faint">{paging.label}</span>
        <Button
          variant="outline"
          size="icon"
          aria-label="Previous page"
          disabled={!paging.canPrevious}
          onclick={() => paging.previous()}
        >‹</Button>
        <Button
          variant="outline"
          size="icon"
          aria-label="Next page"
          disabled={!paging.canNext}
          onclick={() => paging.next()}
        >›</Button>
      </div>
    {/if}
  </footer>
</main>
