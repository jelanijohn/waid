<script lang="ts">
  // The toolbar's settings button. The settings themselves live in
  // SettingsModal — a centered dialog window — rather than an anchored
  // popover, so the forms get room to breathe.
  import Icon from "./Icon.svelte";
  import SettingsModal from "./SettingsModal.svelte";

  let settingsOpen = $state(false);
</script>

<button
  class="btn-icon"
  title="Settings"
  aria-label="Settings"
  onclick={() => (settingsOpen = true)}
>
  <Icon name="tune" size={16} />
</button>

<SettingsModal open={settingsOpen} onclose={() => (settingsOpen = false)} />
