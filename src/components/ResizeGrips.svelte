<script lang="ts">
  /*
   * Eight invisible strips around the window's edge.
   *
   * Not decoration and not optional: an undecorated window has no system resize border, so
   * without these the window can be dragged and maximised and never resized again. They are
   * the half of a custom frame that is easy to forget, because nothing about the window
   * *looks* wrong until somebody reaches for an edge.
   *
   * 5px of reach on the sides, 11px square at the corners -- the system frame's own numbers
   * at 100% scale, near enough. They sit above everything (`z-index` over the dialogs) for
   * the same reason the real border does: a modal must not be able to make a window
   * un-resizable.
   */
  import { chrome } from "$lib/chrome.svelte";

  /** Tauri's `ResizeDirection` strings. */
  const EDGES = [
    { dir: "North", cls: "n" },
    { dir: "South", cls: "s" },
    { dir: "East", cls: "e" },
    { dir: "West", cls: "w" },
    { dir: "NorthEast", cls: "ne" },
    { dir: "NorthWest", cls: "nw" },
    { dir: "SouthEast", cls: "se" },
    { dir: "SouthWest", cls: "sw" }
  ];

  function grab(e: PointerEvent, dir: string) {
    // Left button only: a right-click near an edge is a context menu, not a resize.
    if (e.button !== 0) return;
    e.preventDefault();
    chrome.resizeFrom(dir);
  }
</script>

{#if chrome.available && chrome.on && !chrome.maximized}
  {#each EDGES as g (g.dir)}
    <div class="wgrip {g.cls}" role="presentation" onpointerdown={(e) => grab(e, g.dir)}></div>
  {/each}
{/if}

<style>
  /* `wgrip`, not `grip`: `.grip` is already the per-row drag handle in ModList. Svelte's
     scoping keeps the styles apart, but the name does not, and anything reading the DOM --
     a loadtest, a screen reader tree, a future selector -- sees twenty-three of one and
     eight of the other under one word. */
  .wgrip { position: fixed; z-index: 9999; touch-action: none; }
  .wgrip.n { top: 0; left: 11px; right: 11px; height: 5px; cursor: ns-resize; }
  .wgrip.s { bottom: 0; left: 11px; right: 11px; height: 5px; cursor: ns-resize; }
  .wgrip.w { left: 0; top: 11px; bottom: 11px; width: 5px; cursor: ew-resize; }
  .wgrip.e { right: 0; top: 11px; bottom: 11px; width: 5px; cursor: ew-resize; }
  .wgrip.nw { top: 0; left: 0; width: 11px; height: 11px; cursor: nwse-resize; }
  .wgrip.se { bottom: 0; right: 0; width: 11px; height: 11px; cursor: nwse-resize; }
  .wgrip.ne { top: 0; right: 0; width: 11px; height: 11px; cursor: nesw-resize; }
  .wgrip.sw { bottom: 0; left: 0; width: 11px; height: 11px; cursor: nesw-resize; }
</style>
