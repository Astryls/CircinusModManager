<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { headline } from "$lib/describe";
  import { I } from "$lib/icons";
  import { primaryUid } from "$lib/types";

  const head = $derived(headline(store.issues, store.byUid));
  function review() {
    const target = store.issues.find((i) => i.kind === (head?.kind === "error" ? "incompatible" : "misplacedOptimization")) ?? store.issues[0];
    const uid = target && primaryUid(target);
    if (uid) store.scrollTo(uid);
  }
</script>

{#if head}
  <div class="banner {head.kind}" role="status">
    <span class="ico">{@html head.kind === "error" ? I.error : head.kind === "warning" ? I.warn : I.note}</span>
    <span class="t">{head.title}<small>{head.detail}</small></span>
    <button onclick={review}>Review</button>
  </div>
{/if}

<style>
  .banner { display: flex; align-items: center; gap: 12px; border-radius: 12px; padding: 10px 14px; font-weight: 700; font-size: 13.5px; box-shadow: var(--shadow-raised); }
  .banner.note, .banner.warning { background: var(--amber-deep); color: var(--amber-ink); }
  .banner.error { background: #b8433c; color: #fff4f3; }
  .ico { width: 24px; height: 24px; border-radius: 50%; background: rgba(0, 0, 0, 0.18); display: grid; place-items: center; flex: none; }
  .ico :global(svg) { width: 12px; height: 12px; }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .t small { display: block; font-weight: 500; opacity: 0.8; font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  button { font-weight: 800; padding: 6px 10px; border-radius: 8px; background: rgba(0, 0, 0, 0.16); color: inherit; }
  button:hover { background: rgba(0, 0, 0, 0.26); }
</style>
