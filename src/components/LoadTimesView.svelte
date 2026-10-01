<script lang="ts">
  /*
   * What start-up actually cost, on this machine and on everyone else's.
   *
   * Two sources and neither is ours. The per-mod figures on the left come from the **Loading
   * Progress** mod by ilyvion, which is the only thing in the stack that times a start-up;
   * Circinus reads the file it writes. The figures on the right are those same measurements
   * from other players, pooled by circinus.sh -- so both columns are that mod's work, once
   * locally and once at scale.
   *
   * THE COMPARISON IS THE HARD PART AND IT IS WHY THIS PAGE EXISTS RATHER THAN TWO COLUMNS.
   * Raw milliseconds are not comparable across machines: a slower disk makes every mod slower,
   * so putting "you: 1,400 ms" beside "everyone: 900 ms" on forty-four rows says one thing
   * forty-four times, and that thing is about the computer. The per-mod ratio has the same
   * problem until you notice that the machine IS the median of those ratios -- so dividing each
   * mod's ratio by that median takes the computer out and leaves the mod. A list where
   * everything sits at 1.0x after that is a list with nothing wrong in it, however slow the
   * machine is, and the row at 3.4x is worth reading whichever way the machine leans. It is the
   * same trick `loadCalibration` plays against the model, for the same reason.
   *
   * A missing figure is never a zero. "Nobody has timed this" and "this costs nothing" are
   * opposite claims, and the second one is the one a blank cell quietly makes if you let it: a
   * null sorts last here rather than as the fastest mod in the list.
   */
  import { store } from "$lib/store.svelte";
  import { t } from "$lib/i18n.svelte";
  import { I } from "$lib/icons";
  import { openUrl } from "$lib/api";

  type Sort = "outlier" | "mine" | "theirs" | "name";
  let sort = $state<Sort>("outlier");
  let onlyBoth = $state(false);

  const factor = $derived(store.loadMachineFactor);

  const rows = $derived.by(() => {
    const out = store.loadCompare.map((r) => ({
      ...r,
      // How far this mod is from what this machine does to everything. Null whenever the
      // machine's own factor is unknown, because without it the number would be the disk.
      rel: r.ratio != null && factor != null && factor > 0 ? r.ratio / factor : null
    }));
    const wanted = onlyBoth ? out.filter((r) => r.ratio != null) : out;
    const by: Record<Sort, (a: (typeof out)[0], b: (typeof out)[0]) => number> = {
      outlier: (a, b) => (b.rel ?? -1) - (a.rel ?? -1),
      mine: (a, b) => (b.mine ?? -1) - (a.mine ?? -1),
      theirs: (a, b) => (b.theirs ?? -1) - (a.theirs ?? -1),
      name: (a, b) => a.name.localeCompare(b.name)
    };
    return [...wanted].sort(by[sort]);
  });

  // Per-mod figures are milliseconds and seconds, because that is the scale a mod lives at.
  const secs = (ms: number) => (ms >= 1000 ? `${(ms / 1000).toFixed(ms >= 10_000 ? 0 : 1)} s` : `${Math.round(ms)} ms`);
  /** The three totals, which are minutes. "463 s" is a number you have to convert before it
   *  means anything; the summary card already says 7m 43s and these must agree with it. */
  const clock = (ms: number) => {
    const s = ms / 1000;
    return s >= 60 ? `${Math.floor(s / 60)}m ${Math.round(s % 60)}s` : `${Math.round(s)}s`;
  };
  const when = (ts: number) => (ts ? new Date(ts * 1000).toLocaleString() : "—");
  const mult = (x: number) => `${x.toFixed(x >= 10 ? 0 : x >= 1 ? 1 : 2)}×`;

  const impact = $derived(store.startupImpact);
  const vanillaMs = $derived(impact ? Math.max(0, impact.totalMs - impact.mods.reduce((a, m) => a + m.totalMs, 0)) : 0);
</script>

<main class="center">
  <section class="card">
    <h3>{t("load.title")} <span class="aside">{t("load.by")}</span></h3>

    {#if !impact}
      <p class="lead">{t("load.none")}</p>
      {#if !store.loadTracking.installed}
        <p class="fine">{t("load.track.missing")}</p>
      {:else if !store.loadTrackingOn}
        <p class="fine">{t("load.track.hint")}</p>
      {:else if !store.loadTracking.active}
        <p class="fine">{t("load.track.inactive")}</p>
      {:else}
        <p class="fine">{t("load.track.waiting")}</p>
      {/if}
    {:else}
      <div class="tot">
        <div class="fig"><b>{clock(impact.totalMs)}</b><span>{t("load.total")}</span></div>
        <div class="fig"><b>{clock(vanillaMs)}</b><span>{t("load.vanilla")}</span></div>
        <div class="fig"><b>{impact.mods.length.toLocaleString()}</b><span>{t("load.timed", { n: impact.mods.length, m: store.measuredCount })}</span></div>
      </div>
      {#if impact.defsParsed || impact.patchOps}
        <p class="fine">{t("load.counts", { defs: (impact.defsParsed ?? 0).toLocaleString(), ops: (impact.patchOps ?? 0).toLocaleString() })}</p>
      {/if}
    {/if}

    <div class="acts">
      {#if store.loadTracking.installed}
        <button class="btn" class:primary={!store.loadTrackingOn} onclick={() => store.setLoadTracking(!store.loadTrackingOn)} disabled={!!store.busy}>{@html I.timer}{store.loadTrackingOn ? t("load.track.on") : t("load.track.off")}</button>
      {/if}
      <button class="btn" onclick={() => store.rereadLoadRun()} disabled={!!store.busy}>{@html I.refresh}{t("load.reread")}</button>
      <button class="btn" onclick={() => store.refreshWeights()} disabled={!!store.busy}>{@html I.cloud}{t("load.refetch")}</button>
    </div>
    <p class="fine">{t("load.stamps", { read: when(store.startupImpactAt), fetched: when(store.snap?.weightsFetchedAt ?? 0) })}</p>
  </section>

  <section class="card">
    <h3>{t("load.compare.title")} <span class="aside">{t("load.compare.count", { n: store.loadCompareCount })}</span></h3>

    {#if factor == null}
      <p class="lead">{t("load.compare.thin")}</p>
    {:else}
      <p class="lead">{t("load.compare.factor", { k: mult(factor), n: store.loadCompareCount })}</p>
      <p class="fine">{t("load.compare.pooled")}</p>
    {/if}

    <div class="ctl">
      <div class="seg">
        <button class:on={sort === "outlier"} onclick={() => (sort = "outlier")}>{t("load.sort.outlier")}</button>
        <button class:on={sort === "mine"} onclick={() => (sort = "mine")}>{t("load.sort.mine")}</button>
        <button class:on={sort === "theirs"} onclick={() => (sort = "theirs")}>{t("load.sort.theirs")}</button>
        <button class:on={sort === "name"} onclick={() => (sort = "name")}>{t("load.sort.name")}</button>
      </div>
      <label class="switch"><input type="checkbox" bind:checked={onlyBoth} />{t("load.onlyboth")}</label>
    </div>

    <div class="tbl" role="table" aria-label={t("load.aria.table")}>
      <div class="hd" role="row">
        <span role="columnheader">{t("load.col.mod")}</span>
        <span role="columnheader" class="r">{t("load.col.mine")}</span>
        <span role="columnheader" class="r">{t("load.col.theirs")}</span>
        <span role="columnheader" class="r">{t("load.col.rel")}</span>
      </div>
      {#each rows as r (r.uid)}
        <button class="rw" role="row" onclick={() => { store.view = "order"; store.scrollTo(r.uid); }}>
          <span role="cell" class="nm"><span class="t">{r.name}</span><em>{r.packageId}</em></span>
          <span role="cell" class="r num">{r.mine != null ? secs(r.mine) : "—"}</span>
          <span role="cell" class="r num">
            {#if r.theirs != null}
              {secs(r.theirs)}<em>{t("load.col.runs", { n: r.runs ?? 0 })}</em>
            {:else}
              {"—"}<em>{t("load.notimed")}</em>
            {/if}
          </span>
          <span role="cell" class="r num" class:att={r.rel != null && r.rel >= 2}>{r.rel != null ? mult(r.rel) : "—"}</span>
        </button>
      {:else}
        <p class="lead">{t("load.empty")}</p>
      {/each}
    </div>
  </section>

  <section class="card">
    <h3>{t("load.credit.title")}</h3>
    <p class="fine">{t("load.credit.body")}</p>
    <p class="fine">{t("load.credit.sharing")}</p>
    <button class="btn sm" onclick={() => openUrl("https://steamcommunity.com/sharedfiles/filedetails/?id=3535481557")}>{@html I.link}Loading Progress</button>
  </section>
</main>

<style>
  .center { display: flex; flex-direction: column; gap: 12px; min-width: 0; overflow: auto; padding: 12px; }
  .lead { margin: 0 0 8px; font-size: 13.5px; line-height: 1.5; color: var(--text-2); }
  .fine { margin: 6px 0 0; font-size: 12px; line-height: 1.55; color: var(--text-3); }
  .tot { display: flex; gap: 28px; flex-wrap: wrap; }
  .fig { display: flex; flex-direction: column; gap: 2px; }
  .fig b { font: 700 20px var(--mono); color: var(--text); font-variant-numeric: tabular-nums; }
  .fig span { font-size: 12px; color: var(--text-3); }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 12px; }
  .ctl { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; margin: 10px 0; }

  .tbl { display: flex; flex-direction: column; }
  .hd, .rw { display: grid; grid-template-columns: minmax(0, 1fr) 120px 150px 96px; gap: 12px; align-items: center; padding: 6px 8px; text-align: left; width: 100%; }
  .hd { font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-3); border-bottom: 1px solid var(--surface-3); }
  .rw { min-height: 36px; color: var(--text-2); font-size: 13px; }
  .rw:hover { background: var(--surface-2); color: var(--text); }
  .r { text-align: right; justify-self: end; }
  .num { font-family: var(--mono); font-variant-numeric: tabular-nums; }
  .nm { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .nm .t, .num { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: block; }
  em { font-style: normal; font-size: 11px; color: var(--text-4); font-family: var(--mono); display: block; }
  .att { color: var(--amber); font-weight: 700; }
</style>
