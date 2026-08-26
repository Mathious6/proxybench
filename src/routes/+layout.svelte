<script>
  import "../app.css";

  let { children } = $props();
</script>

<div class="h-full min-h-0">
  {@render children()}
</div>
