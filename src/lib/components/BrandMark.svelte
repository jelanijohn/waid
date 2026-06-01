<script lang="ts">
  // The green "W" brand mark. The single-path logo is rendered via CSS mask so
  // the same file can be recolored per surface (brand green, or status-tinted
  // inside a rock glyph). Defaults to the fixed What's Next green.
  let {
    size = 20,
    color = "var(--wn-brand-green)",
  }: {
    size?: number;
    color?: string;
  } = $props();
</script>

<span
  aria-hidden="true"
  class="inline-block shrink-0"
  style="width: {size}px; height: {size}px; background: {color}; -webkit-mask: url(/logo.svg) center / contain no-repeat; mask: url(/logo.svg) center / contain no-repeat;"
></span>
