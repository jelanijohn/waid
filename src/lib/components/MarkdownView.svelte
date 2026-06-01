<script lang="ts">
  import { renderMarkdown } from "$lib/markdown";
  import { projects } from "$lib/stores/projects.svelte";

  let { source }: { source: string } = $props();

  // Sanitized in renderMarkdown (marked -> DOMPurify), so {@html} is safe here.
  // The resolver marks which `[[wikilinks]]` point at a known brief.
  let html = $derived(
    renderMarkdown(source, (target) => projects.resolveWikilink(target) !== null),
  );

  // Event-delegated wikilink navigation: a click on a resolved `[[link]]`
  // selects that brief instead of trying to open a (non-existent) URL.
  function onClick(e: MouseEvent) {
    const anchor = (e.target as HTMLElement).closest<HTMLElement>("[data-wikilink]");
    if (!anchor) return;
    e.preventDefault();
    const brief = projects.resolveWikilink(anchor.dataset.wikilink ?? "");
    if (brief) projects.select(brief.path);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="markdown" onclick={onClick}>
  {@html html}
</div>
