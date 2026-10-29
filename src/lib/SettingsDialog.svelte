<script lang="ts">
  import { onMount, untrack } from "svelte";
  import type { NumericSetting, Settings, SettingsSheet } from "./ipc";

  type Draft = { target: string; autoCheck: boolean } & Record<NumericSetting, string>;
  type FieldName = "target" | NumericSetting;

  let {
    sheet,
    onSave,
    onClose,
  }: {
    sheet: SettingsSheet;
    onSave: (settings: Settings) => Promise<string | null>;
    onClose: () => void;
  } = $props();

  function draftOf(source: Settings): Draft {
    return {
      target: source.target,
      timeoutSeconds: String(source.timeoutSeconds),
      inFlight: String(source.inFlight),
      startsPerSecond: String(source.startsPerSecond),
      autoCheck: source.autoCheck,
      checkSample: String(source.checkSample),
    };
  }

  let dialog = $state<HTMLDialogElement | null>(null);
  let draft = $state<Draft>(untrack(() => draftOf(sheet.current)));
  let touched = $state<Partial<Record<FieldName, boolean>>>({});
  let saving = $state(false);
  let failure = $state<string | null>(null);

  function rangeText(name: NumericSetting): string {
    const { min, max } = sheet.limits[name];
    return `${min} to ${max}`;
  }

  function numberError(name: NumericSetting): string | null {
    const { min, max } = sheet.limits[name];
    const text = draft[name].trim();
    const value = /^\d+$/.test(text) ? Number(text) : Number.NaN;
    return value >= min && value <= max ? null : `Use ${rangeText(name)}.`;
  }

  function targetError(): string | null {
    return draft.target.trim() === "" ? "Required." : null;
  }

  const errors = $derived<Record<FieldName, string | null>>({
    target: targetError(),
    timeoutSeconds: numberError("timeoutSeconds"),
    inFlight: numberError("inFlight"),
    startsPerSecond: numberError("startsPerSecond"),
    checkSample: numberError("checkSample"),
  });

  const parsed = $derived<Settings>({
    target: draft.target.trim(),
    timeoutSeconds: Number(draft.timeoutSeconds),
    inFlight: Number(draft.inFlight),
    startsPerSecond: Number(draft.startsPerSecond),
    autoCheck: draft.autoCheck,
    checkSample: Number(draft.checkSample),
  });

  const valid = $derived(Object.values(errors).every((error) => error === null));
  const changed = $derived((Object.keys(parsed) as (keyof Settings)[]).some((key) => parsed[key] !== sheet.current[key]));

  onMount(() => {
    dialog?.showModal();
  });

  function visibleError(name: FieldName): string | null {
    return touched[name] ? errors[name] : null;
  }

  function step(event: KeyboardEvent, name: NumericSetting) {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") {
      return;
    }
    event.preventDefault();
    const range = sheet.limits[name];
    const current = Number.parseInt(draft[name], 10);
    const delta = (event.shiftKey ? 10 : 1) * (event.key === "ArrowUp" ? 1 : -1);
    const base = Number.isNaN(current) ? range.min : current;
    draft[name] = String(Math.min(range.max, Math.max(range.min, base + delta)));
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    touched = { target: true, timeoutSeconds: true, inFlight: true, startsPerSecond: true, checkSample: true };
    if (!valid || !changed || saving) {
      return;
    }
    saving = true;
    failure = await onSave(parsed);
    saving = false;
    if (failure === null) {
      dialog?.close();
    }
  }
</script>

{#snippet numeric(name: NumericSetting, label: string, hint: string, suffix: string)}
  {@const disabled = name === "checkSample" && !draft.autoCheck}
  <div class="row">
    <label class="label" for={name}>
      <span>{label}</span>
      <span class="hint" class:error={visibleError(name)} id="{name}-hint">{visibleError(name) ?? hint}</span>
    </label>
    <div class="control">
      <span class="input" class:invalid={visibleError(name)} class:disabled>
        <input
          id={name}
          inputmode="numeric"
          autocomplete="off"
          spellcheck="false"
          {disabled}
          aria-invalid={visibleError(name) !== null}
          aria-describedby="{name}-hint"
          bind:value={draft[name]}
          onblur={() => (touched[name] = true)}
          onkeydown={(event) => step(event, name)}
        />
        {#if suffix}<span class="suffix">{suffix}</span>{/if}
      </span>
    </div>
  </div>
{/snippet}

<dialog
  bind:this={dialog}
  aria-labelledby="settings-title"
  oncancel={(event) => {
    if (saving) {
      event.preventDefault();
    }
  }}
  onclose={onClose}
>
  <form onsubmit={submit} novalidate>
    <h2 id="settings-title">Settings</h2>

    <h3>Probe</h3>
    <div class="group">
      <div class="row stacked">
        <label class="label" for="target">
          <span>Target URL</span>
          <span class="hint" class:error={visibleError("target")} id="target-hint">{visibleError("target") ?? "Headers only. Body is never read."}</span>
        </label>
        <input
          id="target"
          class="field url"
          class:invalid={visibleError("target")}
          type="url"
          placeholder="https://example.com/"
          autocomplete="off"
          spellcheck="false"
          aria-invalid={visibleError("target") !== null}
          aria-describedby="target-hint"
          bind:value={draft.target}
          onblur={() => (touched.target = true)}
        />
      </div>
      {@render numeric("timeoutSeconds", "Timeout", rangeText("timeoutSeconds"), "s")}
      {@render numeric("inFlight", "In flight", rangeText("inFlight"), "")}
      {@render numeric("startsPerSecond", "Start rate", rangeText("startsPerSecond"), "/s")}
    </div>

    <h3>Check</h3>
    <div class="group">
      <div class="row">
        <label class="label" for="auto-check">
          <span>Check on launch and after import</span>
          <span class="hint">CONNECT through a few proxies per /24. No TTFB.</span>
        </label>
        <div class="control">
          <input id="auto-check" class="switch" type="checkbox" role="switch" bind:checked={draft.autoCheck} />
        </div>
      </div>
      {@render numeric("checkSample", "Sample", `Proxies per /24, ${rangeText("checkSample")}`, "each")}
    </div>

    <footer>
      <button type="button" class="link" onclick={() => (draft = draftOf(sheet.defaults))}>Defaults</button>
      {#if failure}
        <p class="failure" title={failure} role="alert"><span class="dot" aria-hidden="true"></span><span class="failure-text">{failure}</span></p>
      {/if}
      <span class="actions">
        <button type="button" class="button" onclick={() => dialog?.close()}>Cancel</button>
        <button type="submit" class="button primary" disabled={!valid || !changed || saving}>Save</button>
      </span>
    </footer>
  </form>
</dialog>

<style>
  dialog {
    width: 512px;
    max-width: calc(100vw - 32px);
    padding: 20px;
    border: 1px solid var(--color-edge);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    color: var(--color-text);
  }

  dialog::backdrop {
    background: rgb(16 16 16 / 0.8);
  }

  h2 {
    margin: 0 0 16px;
    font-size: var(--text-sm);
    font-weight: 500;
  }

  h3 {
    margin: 16px 0 8px;
    color: var(--color-faint);
    font-size: var(--text-xs);
    font-weight: 400;
  }

  h2 + h3 {
    margin-top: 0;
  }

  .group {
    border: 1px solid var(--color-line);
    border-radius: var(--radius-control);
  }

  .row {
    display: grid;
    grid-template-columns: 1fr 112px;
    align-items: center;
    column-gap: 16px;
    padding: 10px 12px;
  }

  .row + .row {
    border-top: 1px solid var(--color-line);
  }

  .row.stacked {
    grid-template-columns: 1fr;
    row-gap: 8px;
  }

  .label {
    display: flex;
    flex-direction: column;
    font-size: var(--text-xs);
    line-height: 16px;
  }

  .hint {
    color: var(--color-faint);
    font-size: var(--text-2xs);
  }

  .control {
    display: flex;
    justify-content: flex-end;
  }

  .url {
    width: 100%;
  }

  .input {
    display: flex;
    align-items: stretch;
    width: 112px;
    height: var(--height-control);
    border: 1px solid var(--color-edge);
    border-radius: var(--radius-control);
    background: var(--color-raised);
    font-size: var(--text-xs);
  }

  .input:focus-within {
    border-color: var(--color-accent);
  }

  .input.disabled {
    opacity: 0.4;
  }

  .input input {
    flex: 1;
    min-width: 0;
    padding: 0 8px;
    border: 0;
    background: none;
    text-align: right;
  }

  .input input:focus {
    outline: none;
  }

  .suffix {
    display: flex;
    align-items: center;
    padding: 0 8px;
    border-left: 1px solid var(--color-edge);
    color: var(--color-faint);
  }

  .invalid,
  .input.invalid {
    border-color: var(--color-bad);
  }

  .hint.error {
    color: var(--color-bad);
  }

  .switch {
    position: relative;
    width: 28px;
    height: 16px;
    margin: 0;
    border: 1px solid var(--color-edge);
    border-radius: 8px;
    background: var(--color-raised);
    appearance: none;
  }

  .switch::after {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--color-faint);
    content: "";
  }

  .switch:checked {
    border-color: var(--color-text);
    background: var(--color-text);
  }

  .switch:checked::after {
    left: 14px;
    background: var(--color-bg);
  }

  @media (prefers-reduced-motion: no-preference) {
    .switch::after {
      transition: left var(--duration-base) var(--ease-ui);
    }
  }

  footer {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 20px;
  }

  .failure {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 8px;
    min-width: 0;
    margin: 0;
    color: var(--color-bad);
    font-size: var(--text-xs);
  }

  .failure-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dot {
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-bad);
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }
</style>
