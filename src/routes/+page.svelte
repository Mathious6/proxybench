<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";
  import type { ImportResult, Progress, RunResult, SubnetRow } from "$lib/import";
  import { emptyMetrics, withMetrics } from "$lib/import";
  import SubnetTable from "$lib/SubnetTable.svelte";
  import UpdateControl from "$lib/UpdateControl.svelte";

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
  let targetField = $state<HTMLInputElement | null>(null);
  let filtering = $state(false);
  let filterCount = $state(0);
  let probeCidr = $state<string | null>(null);
  let pageReset = $state(0);
  let version = $state("");
  let updating = $state(false);
  let paging = $state({
    label: "0 of 0",
    canPrevious: false,
    canNext: false,
    previous: () => {},
    next: () => {},
  });
  let noticeTimer: ReturnType<typeof setTimeout> | null = null;

  const locked = $derived(busy || running || updating);

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

  $effect(() => {
    if (asking) {
      targetField?.focus();
    }
  });

  onMount(() => {
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

  async function exportFiles(cidr?: string) {
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
      const written = await invoke<number>("export_dir", { path, cidr: cidr ?? null });
      showNotice(written === 1 ? "Exported 1 file" : `Exported ${written} files`, false);
    } catch (error) {
      showNotice(String(error), true);
    } finally {
      busy = false;
      working = "";
    }
  }

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
    if (probeCidr) {
      const row = rows.find((item) => item.cidr === probeCidr);
      if (!row) {
        return "";
      }
      const n = row.quantity;
      return `${row.cidr} · ${n.toLocaleString()} ${n === 1 ? "proxy" : "proxies"}`;
    }
    const n = rows.length;
    const proxies = rows.reduce((sum, row) => sum + row.quantity, 0);
    return `${n.toLocaleString()} ${n === 1 ? "subnet" : "subnets"} · ${proxies.toLocaleString()} ${proxies === 1 ? "proxy" : "proxies"}`;
  });

  async function openProbe() {
    if (locked || rows.length === 0) {
      return;
    }
    probeCidr = null;
    asking = true;
  }

  function cancelProbe() {
    asking = false;
    probeCidr = null;
  }

  async function probeSubnet(cidr: string) {
    if (locked) {
      return;
    }
    probeCidr = cidr;
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
    const cidr = probeCidr;
    asking = false;
    running = true;
    clearNotice();
    done = 0;
    const scoped = cidr ? rows.filter((row) => row.cidr === cidr) : rows;
    total = scoped.reduce((sum, row) => sum + row.quantity, 0);
    etaSeconds = null;
    const previous = rows;
    rows = rows.map((row) =>
      (!cidr || row.cidr === cidr) && row.quantity > 0 ? { ...row, ...emptyMetrics() } : row,
    );
    try {
      const probed = await invoke<RunResult>("start_run", { url, cidr });
      const byCidr = new Map(probed.metrics.map((metrics) => [metrics.cidr, metrics]));
      rows = rows.map((row) => {
        const metrics = byCidr.get(row.cidr);
        return metrics ? withMetrics(row, metrics, probed.completedAt) : row;
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
      probeCidr = null;
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
    <div
      class="absolute inset-0 z-20 flex items-center justify-center bg-bg/85 px-6"
      role="presentation"
      onkeydown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          cancelProbe();
        }
      }}
    >
      <form
        class="w-full max-w-md rounded-xl border border-line bg-raised p-5"
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
          class="mt-1 w-full rounded-md border border-line bg-bg px-3 py-2 font-mono text-sm text-text placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
          type="url"
          placeholder="https://example.com/"
          bind:this={targetField}
          bind:value={target}
        />
        <div class="mt-5 flex justify-end gap-2">
          <button
            type="button"
            class="h-7 rounded-md border border-line px-3 text-xs text-muted hover:text-text focus:outline-none focus:ring-1 focus:ring-accent"
            onclick={cancelProbe}
          >
            Cancel
          </button>
          <button
            type="submit"
            class="h-7 rounded-md border border-line bg-raised px-3 text-xs text-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
            disabled={!target.trim()}
          >
            Probe
          </button>
        </div>
      </form>
    </div>
  {/if}
  {#if rows.length === 0}
    <div class="flex min-h-0 flex-1 flex-col items-center justify-center px-6 text-center">
      <h1 class="text-sm font-medium">proxybench</h1>
      <p class="mt-2 text-xs text-faint">Drop a .txt file or a folder of .txt files.</p>
      <button
        type="button"
        class="mt-6 h-7 rounded-md border border-line bg-raised px-3 text-xs text-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
        disabled={locked}
        onclick={pickFiles}
      >
        Open files
      </button>
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
          <button
            type="button"
            class="h-7 rounded-md border border-line px-3 text-xs {filtering
              ? 'bg-raised text-text'
              : 'text-muted hover:text-text'} focus:outline-none focus:ring-1 focus:ring-accent"
            onclick={() => (filtering = !filtering)}
          >
            Filter{filterCount > 0 ? ` ${filterCount}` : ""}
          </button>
          <button
            type="button"
            class="h-7 rounded-md border border-line px-3 text-xs text-muted hover:text-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
            disabled={locked}
            onclick={pickFiles}
          >
            Open files
          </button>
          <button
            type="button"
            class="h-7 rounded-md border border-line px-3 text-xs text-muted hover:text-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
            disabled={locked}
            onclick={() => void exportFiles()}
          >
            Export
          </button>
          <button
            type="button"
            class="h-7 rounded-md border border-line bg-raised px-3 text-xs text-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
            disabled={locked}
            onclick={openProbe}
          >
            Probe all
          </button>
        </div>
      </header>
      <SubnetTable
        {rows}
        {draft}
        {locked}
        {filtering}
        {pageReset}
        onFilterCount={(count) => (filterCount = count)}
        onPageChange={(page) => (paging = page)}
        onAddTag={commitTag}
        onRemoveTag={removeTag}
        onProbeSubnet={probeSubnet}
        onExportSubnet={exportFiles}
        onRemoveSubnet={removeSubnet}
      />
    </div>
  {/if}
  <footer class="flex h-12 shrink-0 items-center border-t border-line px-5">
    <div class="flex shrink-0 items-center gap-1.5 pr-3">
      {#if version}
        <button
          type="button"
          class="rounded-sm text-xs tabular-nums text-faint hover:text-text hover:underline hover:decoration-dotted hover:underline-offset-2 focus:outline-none focus:ring-1 focus:ring-accent"
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
        <button
          type="button"
          class="h-7 w-7 rounded-md border border-line text-xs text-muted hover:text-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
          aria-label="Previous page"
          disabled={!paging.canPrevious}
          onclick={() => paging.previous()}
        >
          ‹
        </button>
        <button
          type="button"
          class="h-7 w-7 rounded-md border border-line text-xs text-muted hover:text-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-40"
          aria-label="Next page"
          disabled={!paging.canNext}
          onclick={() => paging.next()}
        >
          ›
        </button>
      </div>
    {/if}
  </footer>
</main>
