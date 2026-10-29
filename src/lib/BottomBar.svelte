<script lang="ts" module>
  import type { RunKind } from "./ipc";

  export type Activity = { kind: RunKind; done: number; total: number; etaSeconds: number | null };

  export type Notice = { text: string; tone: "info" | "error" };
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import HealthDot from "./HealthDot.svelte";
  import { count, eta } from "./format";
  import { HEALTH_KEYS, type HealthKey } from "./view.svelte";

  let {
    activity,
    notice,
    tally,
    status,
    range,
    canPrevious,
    canNext,
    canSettings,
    version,
    update,
    onStatus,
    onStop,
    onPrevious,
    onNext,
    onSettings,
    onRepository,
  }: {
    activity: Activity | null;
    notice: Notice | null;
    tally: Record<HealthKey, number> | null;
    status: HealthKey | null;
    range: { first: number; last: number; total: number } | null;
    canPrevious: boolean;
    canNext: boolean;
    canSettings: boolean;
    version: string;
    update: Snippet;
    onStatus: (status: HealthKey) => void;
    onStop: () => void;
    onPrevious: () => void;
    onNext: () => void;
    onSettings: () => void;
    onRepository: () => void;
  } = $props();

  const shortcut = /Mac/.test(navigator.userAgent) ? "⌘," : "Ctrl+,";

  const progress = $derived(activity && activity.total > 0 ? Math.min(1, activity.done / activity.total) : 0);
</script>

<footer class="bar">
  {#if activity}
    <span class="progress" style:width="{progress * 100}%" aria-hidden="true"></span>
  {/if}
  <div class="activity">
    {#if activity}
      <span class="run" role="status">
        {activity.kind === "check" ? "Checking" : "Probing"}
        <span class="numbers">{count(activity.done)}/{count(activity.total)}</span>
        {#if activity.kind === "probe" && activity.etaSeconds !== null}
          <span class="faint">{eta(activity.etaSeconds)}</span>
        {/if}
      </span>
      <button type="button" class="link stop" onclick={onStop}>Stop</button>
    {:else if tally}
      <div class="tally" role="group" aria-label="Filter by health">
        {#each HEALTH_KEYS as key (key)}
          {#if tally[key] > 0 || status === key}
            <button
              type="button"
              class="status"
              class:active={status === key}
              aria-pressed={status === key}
              aria-label="{tally[key]} {key}"
              onclick={() => onStatus(key)}
            >
              <HealthDot state={key} />
              <span class="value">{count(tally[key])}</span>
              <span class="name">{key}</span>
            </button>
          {/if}
        {/each}
      </div>
    {/if}
    {#if notice}
      <p class="notice {notice.tone}" title={notice.text} role={notice.tone === "error" ? "alert" : "status"}>
        {#if notice.tone === "error"}<span class="error-dot" aria-hidden="true"></span>{/if}
        <span class="notice-text">{notice.text}</span>
      </p>
    {/if}
  </div>
  <div class="meta">
    {#if range}
      <span class="range">{count(range.first)}–{count(range.last)} of {count(range.total)}</span>
      <button type="button" class="link page" aria-label="Previous page" disabled={!canPrevious} onclick={onPrevious}>‹</button>
      <button type="button" class="link page" aria-label="Next page" disabled={!canNext} onclick={onNext}>›</button>
      <span class="divider" aria-hidden="true"></span>
    {/if}
    <button type="button" class="link" disabled={!canSettings} title="Settings ({shortcut})" onclick={onSettings}>Settings</button>
    {#if version}
      <button type="button" class="link" title="Open the repository" onclick={onRepository}>v{version}</button>
    {/if}
    {@render update()}
  </div>
</footer>

<style>
  .bar {
    position: relative;
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    height: calc(var(--height-bar) + 1px);
    padding: 0 var(--gutter);
    border-top: 1px solid var(--color-line);
    font-size: var(--text-xs);
  }

  .progress {
    position: absolute;
    top: -1px;
    left: 0;
    height: 1px;
    background: var(--color-accent);
  }

  @media (prefers-reduced-motion: no-preference) {
    .progress {
      transition: width var(--duration-base) var(--ease-ui);
    }
  }

  .activity {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 16px;
    min-width: 0;
  }

  .run {
    display: inline-flex;
    gap: 8px;
    color: var(--color-muted);
    white-space: nowrap;
  }

  .numbers {
    color: var(--color-text);
  }

  .faint {
    color: var(--color-faint);
  }

  .stop {
    color: var(--color-text);
    text-decoration: underline;
    text-decoration-color: var(--color-edge);
    text-underline-offset: 3px;
  }

  .tally {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-left: -6px;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 6px;
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: none;
    color: var(--color-text);
    font-size: var(--text-xs);
  }

  .status:hover {
    background: var(--color-surface);
  }

  .status.active {
    border-color: var(--color-edge);
    background: var(--color-raised);
  }

  .name {
    color: var(--color-faint);
  }

  .notice {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    margin: 0;
    color: var(--color-faint);
  }

  .notice.error {
    color: var(--color-bad);
  }

  .notice-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .error-dot {
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-bad);
  }

  .meta {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
    color: var(--color-faint);
  }

  .page {
    width: 20px;
    font-size: var(--text-sm);
  }

  .divider {
    width: 1px;
    height: 16px;
    margin: 0 4px;
    background: var(--color-line);
  }
</style>
