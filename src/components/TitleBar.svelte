<script lang="ts">
  /*
   * The window's own caption bar, replacing the one Windows draws.
   *
   * It is full width, above the three columns, and that cost was weighed rather than
   * defaulted to. A caption strip takes 34px from every window for ever, and the entry in
   * CLAUDE.md about the summary strip argues hard against exactly that kind of permanent
   * charge on the mod list. The first attempt therefore put the three buttons at the right
   * end of the search line, which costs nothing -- and that was wrong: the search line is the
   * top of the *content column*, so with the inspector open the buttons sat 340px short of
   * the window's corner, and a caption button that is not in the corner is one people miss.
   * A caption bar is a caption bar. The 34px is what it costs to have the buttons where the
   * hand already goes.
   *
   * Everything in here is a drag region except the buttons: Tauri starts a drag only when the
   * event target itself carries the attribute, so the title, the mark and the empty middle
   * all move the window and the three controls do not.
   */
  import { chrome } from "$lib/chrome.svelte";
  import WindowControls from "./WindowControls.svelte";
</script>

{#if chrome.available && chrome.on}
  <div class="tbar" data-tauri-drag-region>
    <span class="mk" data-tauri-drag-region>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path opacity=".38" d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 4a6 6 0 1 1 0 12 6 6 0 0 1 0-12z" fill="currentColor" />
        <path d="M9.5 9.5h5v5h-5z" fill="currentColor" />
      </svg>
    </span>
    <span class="ttl" data-tauri-drag-region>Circinus Mod Manager</span>
    <span class="fill" data-tauri-drag-region></span>
    <WindowControls />
  </div>
{/if}

<style>
  /* 34px: two pixels under Windows' own 36, because this one carries no icon tray and the
     title is set at the weight the rest of the window uses rather than at the system's. */
  .tbar {
    height: 34px;
    flex: none;
    display: flex;
    align-items: stretch;
    background: var(--bg-2);
    border-bottom: 1px solid var(--surface-3);
    user-select: none;
    -webkit-user-select: none;
  }
  .mk { display: grid; place-items: center; width: 34px; flex: none; color: var(--text-3); }
  .mk :global(svg) { width: 15px; height: 15px; display: block; pointer-events: none; }
  .ttl {
    display: grid;
    align-items: center;
    font: 600 11.5px var(--font);
    letter-spacing: 0.02em;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fill { flex: 1; min-width: 0; }
</style>
