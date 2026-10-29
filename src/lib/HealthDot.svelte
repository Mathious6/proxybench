<script lang="ts">
  import type { HealthKey } from "./view.svelte";

  let { state }: { state: HealthKey | "checking" } = $props();
</script>

<span class="dot {state}" role="img" title={state} aria-label={state}></span>

<style>
  .dot {
    position: relative;
    display: inline-block;
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    overflow: hidden;
    border-radius: 50%;
  }

  .alive {
    background: var(--color-good);
  }

  .dead {
    background: var(--color-bad);
  }

  .partial {
    border: 1.5px solid var(--color-warn);
  }

  .partial::after {
    position: absolute;
    inset: 0 50% 0 0;
    background: var(--color-warn);
    content: "";
  }

  .unknown {
    border: 1px solid var(--color-faint);
  }

  .checking {
    background: var(--color-faint);
  }

  @media (prefers-reduced-motion: no-preference) {
    .checking {
      animation: pulse 1.2s ease-in-out infinite alternate;
    }
  }

  @keyframes pulse {
    from {
      opacity: 0.35;
    }

    to {
      opacity: 1;
    }
  }
</style>
