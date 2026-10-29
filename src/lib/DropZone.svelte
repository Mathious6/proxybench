<script lang="ts">
  let {
    hovering,
    disabled,
    onOpen,
  }: {
    hovering: boolean;
    disabled: boolean;
    onOpen: () => void;
  } = $props();
</script>

<section class="zone" class:hovering aria-label="Import proxies">
  {#if hovering}
    <p class="title">Drop to import</p>
  {:else}
    <p class="title">Drop .txt files or a folder</p>
    <p class="format">host:port:user:pass</p>
    <button type="button" class="button" {disabled} onclick={onOpen}>Open files</button>
  {/if}
</section>

<style>
  .zone {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    margin: var(--gutter);
    border: 1px dashed var(--color-edge);
    border-radius: var(--radius-control);
  }

  .hovering {
    border-color: var(--color-accent);
  }

  p {
    margin: 0;
  }

  .title {
    color: var(--color-text);
  }

  .format {
    margin-bottom: 16px;
    color: var(--color-faint);
    font-size: var(--text-xs);
  }
</style>
