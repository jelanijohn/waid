<script lang="ts">
  import { renderMarkdown, sectionize } from "$lib/markdown";
  import { collapsedSections, setSectionCollapsed } from "$lib/sections";
  import { projects } from "$lib/stores/projects.svelte";

  let {
    source,
    collapseKey = null,
  }: {
    source: string;
    /** When set (a brief path), `##` sections render as collapsible
     *  `<details>` blocks and their open/closed state is remembered under
     *  this key. Omit for plain, non-collapsible rendering. */
    collapseKey?: string | null;
  } = $props();

  // Sanitized in renderMarkdown (marked -> DOMPurify), so {@html} is safe here.
  // The resolver marks which `[[wikilinks]]` point at a known brief.
  // Collapse state is read (not tracked) here on purpose: toggling a section
  // only touches localStorage, so it never re-renders the whole body — the
  // stored state is simply picked up the next time the source changes.
  let html = $derived.by(() => {
    const rendered = renderMarkdown(source, (target) => projects.resolveWikilink(target) !== null);
    if (collapseKey === null) return rendered;
    const collapsed = collapsedSections(collapseKey);
    return sectionize(rendered, (key) => collapsed.has(key));
  });

  // Event-delegated wikilink navigation: a click on a resolved `[[link]]`
  // selects that brief instead of trying to open a (non-existent) URL.
  function onClick(e: MouseEvent) {
    const anchor = (e.target as HTMLElement).closest<HTMLElement>("[data-wikilink]");
    if (!anchor) return;
    e.preventDefault();
    const brief = projects.resolveWikilink(anchor.dataset.wikilink ?? "");
    if (brief) projects.select(brief.path);
  }

  // `toggle` doesn't bubble, so listen in the capture phase to catch every
  // section's open/close from the one container.
  function onToggle(e: Event) {
    if (collapseKey === null) return;
    const details = e.target as HTMLDetailsElement;
    const key = details.dataset?.section;
    if (!key || !details.classList.contains("md-section")) return;
    setSectionCollapsed(collapseKey, key, !details.open);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="markdown" onclick={onClick} ontogglecapture={onToggle}>
  {@html html}
</div>
