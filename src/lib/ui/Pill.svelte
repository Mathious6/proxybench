<script lang="ts">
  import type { Snippet } from "svelte";

  let { children, tone = "neutral", class: className = "" }: { children: Snippet; tone?: "neutral" | "accent" | "country"; class?: string } = $props();

  const tones = {
    neutral: "border-line bg-raised text-text",
    accent: "border-accent/40 bg-accent/10 text-accent",
    country: "border-line bg-bg text-muted",
  };
</script>

<span class="inline-flex h-[22px] max-w-full items-center truncate rounded-[var(--radius-control)] border px-1.5 font-mono text-xs {tones[tone]} {className}">
  {@render children()}
</span>
