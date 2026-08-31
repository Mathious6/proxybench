<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    children,
    variant = "quiet",
    size = "default",
    joined = "none",
    type = "button",
    disabled = false,
    ref = $bindable<HTMLButtonElement | null>(null),
    onclick,
    class: className = "",
    ...attributes
  }: {
    children: Snippet;
    variant?: "quiet" | "outline" | "primary" | "danger";
    size?: "default" | "compact" | "icon" | "narrowIcon";
    joined?: "none" | "start" | "end";
    type?: "button" | "submit" | "reset";
    disabled?: boolean;
    ref?: HTMLButtonElement | null;
    onclick?: (event: MouseEvent) => void;
    class?: string;
    [key: string]: unknown;
  } = $props();

  const variants = {
    quiet: "border-transparent text-muted hover:bg-raised hover:text-text",
    outline: "border-line text-muted hover:border-muted hover:text-text",
    primary: "border-line bg-raised text-text shadow-[inset_0_1px_0_rgb(255_255_255_/_0.06)] hover:border-accent/70 hover:bg-accent/10",
    danger: "border-bad/50 text-bad hover:bg-bad/10",
  };
  const sizes = {
    default: "h-7 px-3 text-xs",
    compact: "h-6 px-2 text-xs",
    icon: "h-7 w-7 text-sm",
    narrowIcon: "h-7 w-6 text-xs",
  };
  const joins = {
    none: "rounded-[var(--radius-control)]",
    start: "rounded-l-[var(--radius-control)] rounded-r-none",
    end: "rounded-l-none rounded-r-[var(--radius-control)] border-l-0",
  };
</script>

<button
  bind:this={ref}
  {...attributes}
  {type}
  {disabled}
  {onclick}
  class="inline-flex shrink-0 items-center justify-center border font-medium outline-none focus-visible:ring-1 focus-visible:ring-accent disabled:pointer-events-none disabled:opacity-40 {variants[variant]} {sizes[size]} {joins[joined]} {className}"
>
  {@render children()}
</button>
