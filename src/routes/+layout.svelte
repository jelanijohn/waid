<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { settings } from "$lib/stores/settings.svelte";

  let { children } = $props();

  onMount(() => {
    settings.init();
  });
</script>

{@render children()}
