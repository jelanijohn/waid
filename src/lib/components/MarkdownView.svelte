<script lang="ts">
  import { renderMarkdown } from "$lib/markdown";

  let { source }: { source: string } = $props();

  // Sanitized in renderMarkdown (marked -> DOMPurify), so {@html} is safe here.
  let html = $derived(renderMarkdown(source));
</script>

<div class="markdown text-slate-700 dark:text-slate-300">
  {@html html}
</div>
