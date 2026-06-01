<script lang="ts">
  // A single Material Symbols Rounded glyph. The variable font is imported in
  // app.css; `fill`/`weight`/`size` map straight onto its axes.
  let {
    name,
    size = 18,
    fill = 0,
    weight = 400,
    class: klass = "",
  }: {
    name: string;
    size?: number;
    fill?: 0 | 1;
    weight?: number;
    class?: string;
  } = $props();
</script>

<span
  class="msym {klass}"
  aria-hidden="true"
  style="font-size: {size}px; font-variation-settings: 'FILL' {fill}, 'wght' {weight}, 'GRAD' 0, 'opsz' {size};"
>{name}</span>
