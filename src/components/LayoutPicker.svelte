<script lang="ts">
  /*
   * How one list is drawn: one run, or broken into bands.
   *
   * Three of these can be on screen at once -- the single list, and the two panes of the split
   * -- because they answer different questions and somebody can want different answers at the
   * same time: A to Z on the left to find a mod, phases on the right to check the order.
   *
   * The words differ by surface and that is deliberate. The active list has a load order, so
   * its flat layout is "Load order". The inactive pane has none, so calling its flat layout
   * that would be a lie about a list sorted by name; there it reads "A to Z". The stored value
   * is the same either way, because the choice is between one run and bands, not between two
   * words.
   */
  import { layouts, type Layout, type Surface } from "$lib/layout.svelte";
  import { t } from "$lib/i18n.svelte";

  let { surface, compact = false }: { surface: Surface; compact?: boolean } = $props();

  const flatLabel = $derived(surface === "inactive" ? "A–Z" : "Load order");
  const flatShort = $derived(surface === "inactive" ? "A–Z" : "Order");
  const flatTitle = $derived(
    surface === "inactive"
      ? "One run, by name. An inactive mod has no place in the load order, so there is no order to show it in."
      : "One run, in the order the game will load them, exactly as ModsConfig.xml has it."
  );
  const now = $derived(layouts.of(surface));
  const pick = (l: Layout) => now !== l && layouts.set(surface, l);
</script>

<div class="seg lay" class:compact role="radiogroup" aria-label={t("toolbar.arrangement")}>
  <button role="radio" class:on={now === "flat"} aria-checked={now === "flat"} title={flatTitle} onclick={() => pick("flat")}>
    <span class="lg">{flatLabel}</span><span class="sm">{flatShort}</span>
  </button>
  <button
    role="radio"
    class:on={now === "phase"}
    aria-checked={now === "phase"}
    title={t("toolbar.arr.phase.title")}
    onclick={() => pick("phase")}
  >
    <span class="lg">{t("toolbar.arr.phase")}</span><span class="sm">{t("toolbar.arr.phase.short")}</span>
  </button>
</div>

<style>
  .lay button { font-size: 12px; padding: 0 10px; }
  .lay .sm { display: none; }
  /* A pane header has half the width and the pane's own name already in it, so the short
     labels are the right ones there whatever the width. */
  .lay.compact { padding: 2px; }
  .lay.compact button { height: 22px; padding: 0 8px; font-size: 11.5px; }
  .lay.compact .lg { display: none; }
  .lay.compact .sm { display: inline; }
  /* In the toolbar the words give way at the same width everything else does. */
  @container (max-width: 980px) { .lay button { padding: 0 8px; } }
  @container (max-width: 800px) { .lay .lg { display: none; } .lay .sm { display: inline; } }
</style>
