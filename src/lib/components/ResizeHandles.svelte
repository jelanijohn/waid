<script lang="ts">
  // With OS decorations off (Windows/Linux/WSL — see lib.rs) the borderless,
  // transparent webview fills the whole window, so there is no native frame edge
  // to grab for resizing. We lay invisible grips along the window edges and hand
  // the drag to the OS via the Tauri startResizeDragging API.
  //
  // Crucial detail for transparent windows on Linux/WSL: pointer events over the
  // fully transparent 14px gutter pass straight through to the window behind, so
  // a grip floating in the gutter never gets hovered. Each grip therefore
  // straddles the gutter AND reaches a few px onto the opaque panel, so its inner
  // band always captures the cursor. On Windows there is no gutter (the panel
  // fills the window, see lib/platform.ts), so the grips are a thin band at the
  // window edge instead. The side grips start below the 46px titlebar so they
  // never sit over the caption buttons. macOS keeps its native frame, so we skip
  // all of this there.
  import { getCurrentWindow, type CursorIcon } from "@tauri-apps/api/window";
  import { platform, paintsOwnFrame } from "$lib/platform";

  const isMac = platform === "macos";
  const win = getCurrentWindow();

  type Dir =
    | "North" | "South" | "East" | "West"
    | "NorthEast" | "NorthWest" | "SouthEast" | "SouthWest";

  // WebKitGTK (Linux/WSL) often ignores the CSS `cursor` on these transparent
  // overlay divs, so we also drive the native cursor through Tauri on hover. The
  // CSS cursor stays as the Windows fallback.
  const CURSOR: Record<Dir, CursorIcon> = {
    North: "nsResize", South: "nsResize",
    East: "ewResize", West: "ewResize",
    NorthWest: "nwseResize", SouthEast: "nwseResize",
    NorthEast: "neswResize", SouthWest: "neswResize",
  };

  function enter(dir: Dir) {
    // No-op if window APIs are unavailable (e.g. plain `vite dev` in a browser).
    win.setCursorIcon(CURSOR[dir]).catch(() => {});
  }
  function leave() {
    win.setCursorIcon("default").catch(() => {});
  }
  function start(dir: Dir, e: MouseEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    win.startResizeDragging(dir).catch(() => {});
  }
</script>

{#if !isMac}
<div class="rh-root" data-gutter={paintsOwnFrame}>
  <!-- Top edge — left half only, so it clears the right-side action/caption
       buttons. (No top-right grip: the close button lives there.) -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rh edge-n" onmouseenter={() => enter("North")} onmouseleave={leave} onmousedown={(e) => start("North", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rh corner-nw" onmouseenter={() => enter("NorthWest")} onmouseleave={leave} onmousedown={(e) => start("NorthWest", e)}></div>
  <!-- Left / right edges (below the titlebar so they clear the caption buttons). -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rh edge-w" onmouseenter={() => enter("West")} onmouseleave={leave} onmousedown={(e) => start("West", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rh edge-e" onmouseenter={() => enter("East")} onmouseleave={leave} onmousedown={(e) => start("East", e)}></div>
  <!-- Bottom edge + bottom corners. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rh edge-s" onmouseenter={() => enter("South")} onmouseleave={leave} onmousedown={(e) => start("South", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rh corner-sw" onmouseenter={() => enter("SouthWest")} onmouseleave={leave} onmousedown={(e) => start("SouthWest", e)}></div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rh corner-se" onmouseenter={() => enter("SouthEast")} onmouseleave={leave} onmousedown={(e) => start("SouthEast", e)}></div>
</div>
{/if}

<style>
  /* Grip thickness. With a transparent gutter (Linux/WSL) each grip spans 24px
     in from the window edge: the outer 14px overlaps the gutter, the inner ~10px
     overlaps the opaque panel border — that inner band is what reliably captures
     the cursor. Without a gutter (Windows) the panel is opaque right up to the
     edge, so a 6px band is enough and keeps the grips off the content. */
  .rh-root {
    --grip: 6px;
    --side-top: 46px;
    display: contents;
  }
  .rh-root[data-gutter="true"] {
    --grip: 24px;
    --side-top: 64px;
  }
  .rh {
    position: fixed;
    z-index: 100;
  }
  .edge-n {
    top: 0;
    left: var(--grip);
    right: 50%;
    height: var(--grip);
    cursor: ns-resize;
  }
  .corner-nw {
    top: 0;
    left: 0;
    width: var(--grip);
    height: var(--grip);
    cursor: nwse-resize;
  }
  .edge-w {
    left: 0;
    width: var(--grip);
    top: var(--side-top);
    bottom: var(--grip);
    cursor: ew-resize;
  }
  .edge-e {
    right: 0;
    width: var(--grip);
    top: var(--side-top);
    bottom: var(--grip);
    cursor: ew-resize;
  }
  .edge-s {
    bottom: 0;
    height: var(--grip);
    left: var(--grip);
    right: var(--grip);
    cursor: ns-resize;
  }
  .corner-sw {
    bottom: 0;
    left: 0;
    width: var(--grip);
    height: var(--grip);
    cursor: nesw-resize;
  }
  .corner-se {
    bottom: 0;
    right: 0;
    width: var(--grip);
    height: var(--grip);
    cursor: nwse-resize;
  }
</style>
