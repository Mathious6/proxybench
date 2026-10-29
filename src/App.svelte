<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import BottomBar, { type Activity, type Notice } from "./lib/BottomBar.svelte";
  import DropZone from "./lib/DropZone.svelte";
  import Menu, { type MenuEntry } from "./lib/Menu.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import SubnetTable from "./lib/SubnetTable.svelte";
  import Toolbar from "./lib/Toolbar.svelte";
  import UpdateControl from "./lib/UpdateControl.svelte";
  import { plural } from "./lib/format";
  import { ipc, type DragDrop, type RunKind, type RunProgress, type Settings, type SettingsSheet, type SubnetRow } from "./lib/ipc";
  import { View } from "./lib/view.svelte";

  type OpenMenu = { x: number; y: number; label: string; entries: MenuEntry[]; trigger: HTMLElement };

  let rows = $state<SubnetRow[]>([]);
  let loaded = $state(false);
  let sheet = $state<SettingsSheet | null>(null);
  let activity = $state<Activity | null>(null);
  let notice = $state<Notice | null>(null);
  let busy = $state(false);
  let updating = $state(false);
  let hovering = $state(false);
  let settingsOpen = $state(false);
  let version = $state("");
  let menu = $state<OpenMenu | null>(null);
  let filterField = $state<HTMLInputElement | null>(null);

  const pending = new SvelteMap<string, number | null>();
  const view = new View(() => rows);
  let runToken = 0;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  const probing = $derived(activity?.kind === "probe");
  const idle = $derived(!busy && !updating);
  const canImport = $derived(idle && !probing);
  const canExport = $derived(idle && !probing && rows.length > 0);
  const canEdit = $derived(idle && activity === null);
  const probeLabel = $derived(probing ? "Stop" : view.selected.size > 0 ? `Probe ${view.selected.size}` : "Probe all");

  function clearNotice() {
    clearTimeout(noticeTimer);
    notice = null;
  }

  function info(text: string) {
    clearTimeout(noticeTimer);
    notice = { text, tone: "info" };
    noticeTimer = setTimeout(() => (notice = null), 4000);
  }

  function fail(error: unknown) {
    clearTimeout(noticeTimer);
    notice = { text: error instanceof Error ? error.message : String(error), tone: "error" };
  }

  function merge(next: SubnetRow[]) {
    const byCidr = new Map(next.map((row) => [row.cidr, row]));
    rows = rows.map((row) => byCidr.get(row.cidr) ?? row);
  }

  async function act(work: () => Promise<void>) {
    busy = true;
    clearNotice();
    try {
      await work();
    } catch (error) {
      fail(error);
    } finally {
      busy = false;
    }
  }

  async function start(kind: RunKind, cidrs: string[] | null) {
    if (activity && (kind === "check" || activity.kind === "probe")) {
      return;
    }
    const scoped = cidrs ? rows.filter((row) => cidrs.includes(row.cidr)) : rows;
    if (scoped.length === 0) {
      return;
    }
    const token = ++runToken;
    pending.clear();
    for (const row of scoped) {
      pending.set(row.cidr, row.probe?.at ?? null);
    }
    activity = {
      kind,
      done: 0,
      total: kind === "check" ? scoped.length : scoped.reduce((sum, row) => sum + row.quantity, 0),
      etaSeconds: null,
    };
    if (notice?.tone === "error") {
      clearNotice();
    }
    try {
      const summary = await (kind === "check" ? ipc.check(cidrs) : ipc.probe(cidrs));
      if (token === runToken) {
        merge(summary.rows);
        const verb = kind === "check" ? "Check" : "Probe";
        if (summary.stopped) {
          info(`${verb} stopped`);
        } else if (kind === "probe" || notice === null) {
          info(`${verb === "Check" ? "Checked" : "Probed"} ${plural(summary.rows.length, "subnet", "subnets")}`);
        }
      }
    } catch (error) {
      if (token === runToken) {
        fail(error);
      }
    } finally {
      if (token === runToken) {
        activity = null;
        pending.clear();
      }
    }
  }

  function progress(update: RunProgress) {
    merge([update.row]);
    if (activity?.kind !== update.kind) {
      return;
    }
    activity = { kind: update.kind, done: update.done, total: update.total, etaSeconds: update.etaSeconds };
    if (update.kind === "check" || (update.row.probe?.at ?? null) !== pending.get(update.row.cidr)) {
      pending.delete(update.row.cidr);
    }
  }

  async function stop() {
    try {
      await ipc.stop();
    } catch (error) {
      fail(error);
    }
  }

  function probeOrStop() {
    if (probing) {
      void stop();
    } else {
      void start("probe", view.scope);
    }
  }

  async function importPaths(paths: string[]) {
    if (!canImport || paths.length === 0) {
      return;
    }
    let fresh: string[] = [];
    await act(async () => {
      const report = await ipc.importPaths(paths);
      rows = report.rows;
      fresh = [...report.added, ...report.grown];
      const parts = [plural(report.added.length, "subnet added", "subnets added")];
      if (report.grown.length > 0) {
        parts.push(`${report.grown.length} grown`);
      }
      if (report.skipped > 0) {
        parts.push(plural(report.skipped, "line skipped", "lines skipped"));
      }
      info(parts.join(" · "));
    });
    if (sheet?.current.autoCheck && activity === null && fresh.length > 0) {
      void start("check", fresh);
    }
  }

  async function openFiles() {
    if (!canImport) {
      if (probing) {
        info("Stop the probe to import");
      }
      return;
    }
    try {
      const paths = await ipc.pickFiles();
      if (paths) {
        await importPaths(paths);
      }
    } catch (error) {
      fail(error);
    }
  }

  async function exportTxt(cidrs: string[] | null) {
    const folder = await ipc.pickFolder().catch(fail);
    if (folder) {
      await act(async () => info(`Exported ${plural(await ipc.exportTxt(folder, cidrs), "file", "files")}`));
    }
  }

  async function exportAycd(cidrs: string[] | null) {
    const path = await ipc.pickAycdFile().catch(fail);
    if (path) {
      await act(async () => info(`Exported ${plural(await ipc.exportAycd(path, cidrs), "proxy", "proxies")} for AYCD`));
    }
  }

  async function remove(cidrs: string[]) {
    await act(async () => {
      rows = await ipc.remove(cidrs);
      view.retain(rows);
      info(`Removed ${plural(cidrs.length, "subnet", "subnets")}`);
    });
  }

  async function setTags(cidr: string, tags: string[]) {
    try {
      const stored = await ipc.setTags(cidr, tags);
      rows = rows.map((row) => (row.cidr === cidr ? { ...row, tags: stored } : row));
    } catch (error) {
      fail(error);
    }
  }

  async function saveSettings(next: Settings): Promise<string | null> {
    try {
      const current = await ipc.saveSettings(next);
      sheet = sheet && { ...sheet, current };
      info("Settings saved");
      return null;
    } catch (error) {
      return error instanceof Error ? error.message : String(error);
    }
  }

  function openSettings() {
    if (canEdit) {
      clearNotice();
      settingsOpen = true;
    } else if (activity) {
      info(`Settings open when the ${activity.kind} ends`);
    }
  }

  function openExportMenu(trigger: HTMLElement) {
    const bounds = trigger.getBoundingClientRect();
    const scope = view.scope;
    menu = {
      x: bounds.left,
      y: bounds.bottom + 4,
      label: "Export",
      trigger,
      entries: [
        { label: "TXT files", tone: "plain", disabled: !canExport, act: () => void exportTxt(scope) },
        { label: "AYCD JSON", tone: "plain", disabled: !canExport, act: () => void exportAycd(scope) },
      ],
    };
  }

  function openRowMenu(cidr: string, x: number, y: number, trigger: HTMLElement) {
    const scope = view.menuScope(cidr);
    const many = scope.length > 1;
    menu = {
      x,
      y,
      label: many ? plural(scope.length, "subnet", "subnets") : cidr,
      trigger,
      entries: [
        { label: "Check", tone: "plain", disabled: !canEdit, act: () => void start("check", scope) },
        { label: "Probe", tone: "plain", disabled: !idle || probing, act: () => void start("probe", scope) },
        { label: "Export TXT", tone: "plain", disabled: !canExport, act: () => void exportTxt(scope) },
        { label: "Export AYCD JSON", tone: "plain", disabled: !canExport, act: () => void exportAycd(scope) },
        {
          label: many ? `Remove ${plural(scope.length, "subnet", "subnets")}` : "Remove",
          tone: "danger",
          disabled: !canEdit,
          act: () => void remove(scope),
        },
      ],
    };
  }

  function drag(event: DragDrop) {
    hovering = event.kind === "over";
    if (event.kind === "drop") {
      void importPaths(event.paths);
    }
  }

  function keydown(event: KeyboardEvent) {
    if (settingsOpen || menu) {
      return;
    }
    const modified = event.metaKey || event.ctrlKey;
    const key = event.key.toLowerCase();
    const typing = event.target instanceof HTMLInputElement;
    if (modified && key === ",") {
      event.preventDefault();
      openSettings();
    } else if (modified && key === "o") {
      event.preventDefault();
      void openFiles();
    } else if (modified && key === "f") {
      event.preventDefault();
      filterField?.focus();
      filterField?.select();
    } else if (modified && key === "a" && !typing) {
      event.preventDefault();
      view.selectFiltered();
    } else if (key === "escape" && view.selected.size > 0) {
      view.clearSelection();
    }
  }

  async function load() {
    try {
      const [stored, saved] = await Promise.all([ipc.rows(), ipc.settings()]);
      rows = stored;
      sheet = saved;
      loaded = true;
      if (saved.current.autoCheck && stored.length > 0) {
        void start("check", null);
      }
    } catch (error) {
      loaded = true;
      fail(error);
    }
  }

  onMount(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    function keep(stop: () => void) {
      if (disposed) {
        stop();
      } else {
        stops.push(stop);
      }
    }
    void Promise.all([ipc.onProgress(progress).then(keep), ipc.onDragDrop(drag).then(keep)])
      .catch(fail)
      .then(load);
    ipc.version().then((value) => (version = value), () => {});
    return () => {
      disposed = true;
      clearTimeout(noticeTimer);
      for (const stop of stops) {
        stop();
      }
    };
  });
</script>

<svelte:window onkeydown={keydown} />

<main>
  <Toolbar
    total={rows.length}
    selected={view.selected.size}
    query={view.query}
    bind:filterField
    {canImport}
    {canExport}
    {probeLabel}
    canProbe={probing || (idle && rows.length > 0)}
    onClear={() => view.clearSelection()}
    onQuery={(query) => view.setQuery(query)}
    onOpen={() => void openFiles()}
    onExport={openExportMenu}
    onProbe={probeOrStop}
  />
  <div class="content">
    {#if loaded && rows.length === 0}
      <DropZone {hovering} disabled={!canImport} onOpen={() => void openFiles()} />
    {:else if loaded}
      <SubnetTable {view} checking={(cidr) => pending.has(cidr)} onMenu={openRowMenu} onTags={(cidr, tags) => void setTags(cidr, tags)} />
      {#if view.ordered.length === 0}
        <div class="no-match">
          <p>{view.query.trim() ? `No subnets match "${view.query.trim()}"` : `No ${view.status} subnets`}</p>
          <button type="button" class="button" onclick={() => view.clearFilter()}>Clear filter</button>
        </div>
      {/if}
      {#if hovering}
        <div class="overlay">{probing ? "Stop the probe to import" : "Drop to import"}</div>
      {/if}
    {/if}
  </div>
  <BottomBar
    {activity}
    {notice}
    tally={rows.length > 0 ? view.tally : null}
    status={view.status}
    range={view.ordered.length > 0 ? view.range : null}
    canPrevious={view.currentPage > 0}
    canNext={view.currentPage < view.pageCount - 1}
    canSettings={canEdit}
    {version}
    onStatus={(status) => view.toggleStatus(status)}
    onStop={() => void stop()}
    onPrevious={() => view.previousPage()}
    onNext={() => view.nextPage()}
    onSettings={openSettings}
    onRepository={() => void ipc.openRepository().catch(fail)}
  >
    {#snippet update()}
      <UpdateControl
        disabled={activity !== null || busy}
        onBusy={(value) => (updating = value)}
        onInfo={info}
        onError={fail}
      />
    {/snippet}
  </BottomBar>
  {#if settingsOpen && sheet}
    <SettingsDialog {sheet} onSave={saveSettings} onClose={() => (settingsOpen = false)} />
  {/if}
  {#if menu}
    <Menu x={menu.x} y={menu.y} label={menu.label} entries={menu.entries} trigger={menu.trigger} onDismiss={() => (menu = null)} />
  {/if}
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .content {
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .no-match {
    position: absolute;
    inset: var(--height-head) 0 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--color-muted);
  }

  .no-match p {
    margin: 0;
  }

  .overlay {
    position: absolute;
    inset: 8px var(--gutter);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--color-accent);
    border-radius: var(--radius-control);
    background: rgb(16 16 16 / 0.88);
    color: var(--color-text);
  }
</style>
