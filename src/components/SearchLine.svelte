<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { keyLabel } from "$lib/keys.svelte";
  import { t } from "$lib/i18n.svelte";
  import { I } from "$lib/icons";

  const total = $derived(store.mods.length);

  /** The search box answers for its own keys.
   *
   *  Escape here means "never mind what I typed", which is why it clears before it leaves: a
   *  search box that closed something behind it while leaving the query in place would be
   *  answering a question nobody asked. Empty, there is nothing to undo, so it hands focus back
   *  to the list -- and the list is also where Down and Enter go, because typing a name and then
   *  reaching for the mouse to click the one result is the thing people actually complain about.
   *
   *  This runs before the window handler and stops there, so Escape never reaches the Escape
   *  stack while there is text to clear. */
  function searchKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (store.query) store.query = "";
      else (e.target as HTMLElement).blur();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "Enter") {
      const first = store.visibleList()[0];
      if (!first) return;
      e.preventDefault();
      e.stopPropagation();
      store.scrollTo(first, { select: true, focus: true });
    }
  }
</script>

<!-- The search line sits at the top of the content column rather than in a title bar, because in
     this layout there is no title bar: the directory carries identity and the actions, and what
     is left above the content is the one control that acts on the content. -->
<div class="sline">
  <span class="ic">{@html I.search}</span>
  <input id="search" type="search" placeholder={t("search.placeholder", { n: total })} aria-label={t("search.aria")} bind:value={store.query} onkeydown={searchKey} />
  <kbd>{keyLabel("Mod+k")}</kbd>
</div>

<style>
  .sline { display: flex; align-items: center; gap: 9px; padding: 10px 14px; border-bottom: 1px solid var(--surface-3); flex: none; min-width: 0; }
  .ic { display: grid; place-items: center; flex: none; color: var(--text-3); }
  .ic :global(svg) { width: 15px; height: 15px; display: block; }
  input { flex: 1; min-width: 0; border: 0; background: none; outline: none; color: var(--text); font-family: var(--mono); font-size: 12px; padding: 0; }
  input::placeholder { color: var(--text-3); }
  input::-webkit-search-cancel-button { display: none; }
  kbd { font-family: var(--mono); font-size: 10px; color: var(--text-3); border: 1px solid var(--surface-3); padding: 2px 6px; flex: none; }
</style>
