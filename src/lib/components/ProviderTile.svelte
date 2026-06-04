<script lang="ts">
  // A provider monogram tile in the provider's brand color. Brand colors live
  // in app.css (`.ptile-<provider>`) so dark-mode tweaks (GitHub) stay in CSS.
  import { PROVIDERS } from "$lib/providers";
  import type { Provider } from "$lib/types";

  let { provider, size = 30 }: { provider: Provider; size?: number } = $props();

  let meta = $derived(PROVIDERS[provider]);
</script>

<span
  class="ptile ptile-{provider}"
  aria-hidden="true"
  style="width: {size}px; height: {size}px; font-size: {Math.round(size * 0.46)}px; border-radius: {Math.round(size * 0.28)}px;"
>{meta.mono}</span>
