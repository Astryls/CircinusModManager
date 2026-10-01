<script lang="ts">
  /*
   * Minimise, maximise, close -- in the window's own colours.
   *
   * They sit at the right-hand end of `TitleBar`, which is the window's own corner. The
   * first version put them on the search line instead, to avoid spending 34px on a caption
   * bar -- and that put them at the end of the *content column*, 340px short of the corner
   * whenever the inspector was open. See TitleBar for why the 34px is worth paying.
   *
   * Drawn from the same 24-grid rectangles as `lib/icons.ts` rather than from the Segoe
   * Fluent Icons font Windows uses. The font is not on every machine the app runs on, and a
   * caption glyph that falls back to a tofu box is worse than one that is a pixel off the
   * system's.
   */
  import { chrome } from "$lib/chrome.svelte";
  import { t } from "$lib/i18n.svelte";

  /** The maximise button, watched so Rust always knows where it is. It moves with the
   *  window's width and with the display scale, and a hit rectangle a scale change behind is
   *  a Snap Layouts flyout that opens over empty bar. */
  let maxBtn = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    if (!maxBtn) return;
    const report = () => chrome.reportMaximiseRect(maxBtn);
    report();
    const ro = new ResizeObserver(report);
    ro.observe(maxBtn);
    window.addEventListener("resize", report);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", report);
    };
  });
</script>

{#if chrome.available && chrome.on}
  <div class="wc">
    <button class="wb" title={t("window.minimise")} aria-label={t("window.minimise")} onclick={() => chrome.minimize()}>
      <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M5 11h14v2H5z" /></svg>
    </button>
    <!-- `class:hot` rather than `:hover`: the native hit test has taken this rectangle into
         the caption, so the webview no longer gets a mouse event for it. Rust forwards the
         non-click part and this draws it. -->
    <button
      class="wb"
      class:hot={chrome.snapHover}
      bind:this={maxBtn}
      title={chrome.maximized ? t("window.restore") : t("window.maximise")}
      aria-label={chrome.maximized ? t("window.restore") : t("window.maximise")}
      onclick={() => chrome.toggleMaximize()}
    >
      {#if chrome.maximized}
        <!-- Two sheets, the way the system draws a restore: the one behind stepped back, so
             the state reads without the tooltip. -->
        <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path opacity=".45" d="M8 4h12v12h-3V7H8z" />
          <path d="M4 8h12v12H4zm2 2v8h8v-8z" />
        </svg>
      {:else}
        <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M5 5h14v14H5zm2 2v10h10V7z" /></svg>
      {/if}
    </button>
    <!-- The only one that gets a colour, and only on hover. Red means an error everywhere
         else in this window; here it means the press you cannot take back. -->
    <button class="wb bad" title={t("window.close")} aria-label={t("window.close")} onclick={() => chrome.close()}>
      <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M5.6 4.2l14.2 14.2-1.4 1.4L4.2 5.6zM18.4 4.2l1.4 1.4L5.6 19.8l-1.4-1.4z" /></svg>
    </button>
  </div>
{/if}

<style>
  /* No margins. These used to carry negative ones, to cancel the padding of the search line
     they first lived in; in a caption bar with no padding of its own that pushed them ten
     pixels above the window and fourteen past its right edge. The bar stretches them, and the
     corner is the corner. */
  /* One pixel of overlap onto the bar's bottom rule, so a button reaches the edge it looks
     like it reaches rather than stopping a hairline short of it. */
  .wc { display: flex; align-items: stretch; flex: none; margin-bottom: -1px; }
  .wb {
    display: grid;
    place-items: center;
    width: 44px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text-3);
    cursor: pointer;
    padding: 0;
  }
  .wb :global(svg) { width: 13px; height: 13px; display: block; }
  .wb:hover { background: var(--surface-2); color: var(--text); }
  .wb:active { background: var(--surface-3); }
  .wb.hot { background: var(--surface-2); color: var(--text); }
  .wb.bad:hover { background: var(--red); color: var(--bg); }
  .wb:focus-visible { outline: 1px solid var(--amber); outline-offset: -2px; }
</style>
