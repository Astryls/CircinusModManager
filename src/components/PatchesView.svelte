<script lang="ts">
  import { store, type PatchesCol, type PatchesTab } from "$lib/store.svelte";
  import { t } from "$lib/i18n.svelte";
  import { I } from "$lib/icons";
  import { patchTargetName, splitTarget, type ModPatchDetail, type ModPatches, type TargetGroup } from "$lib/types";

  const job = $derived(store.patchJob);
  const report = $derived(store.patchReport);
  store.refreshPatches();

  const contested = $derived((report?.targets ?? []).filter((t) => t.contested));
  const manualMods = $derived((report?.perMod ?? []).filter((m) => m.manual > 0));
  const unreadable = $derived((report?.perMod ?? []).filter((m) => m.unreadable.length));
  const manualCount = $derived(manualMods.reduce((n, m) => n + m.manual, 0));

  // ---- four questions, four screens ----
  //
  // This was one page: four sections stacked, the long ones cut off at a hundred and twenty rows
  // with a line apologising for the rest. Cutting a list is the wrong answer to a list being long
  // -- the row somebody wants is as likely to be the two thousandth as the second -- and stacking
  // four of them means the fourth is a thousand rows of scrolling away from the first. So each
  // question gets its own screen, each screen is complete, and the tab bar is the map.
  const tab = $derived(store.patchesTab);
  const tabs = $derived.by((): { id: PatchesTab; label: string; count: number | null }[] => [
    { id: "overview", label: t("patches.tab.overview"), count: null },
    { id: "contested", label: t("patches.tab.contested"), count: contested.length },
    { id: "methods", label: t("patches.tab.methods"), count: report?.targets.length ?? 0 },
    { id: "permod", label: t("patches.tab.permod"), count: report?.perMod.length ?? 0 },
    { id: "manual", label: t("patches.tab.manual"), count: manualCount }
  ]);

  // ---- keeping the place ----
  //
  // Switching views destroys this component, so the position has to live somewhere that outlives
  // it -- and clicking a mod here is a switch to the load order, which is exactly the thing
  // somebody does most from this screen. A report does not change while you are away from it, so
  // the pixel is enough here; the mod list needs an anchor because its content can move.
  //
  // Each tab keeps its own pixel: they are different lists, and one number for all four would
  // put the per-mod table where the method list was left.
  //
  // `{#key tab}` gives each tab a scroller of its own rather than reusing one. Sharing it meant
  // the browser clamping the old position against the new tab's height and firing a scroll event
  // for it, which wrote that clamped number over the place the new tab was actually left at.
  const scope = $derived(`patches:${tab}`);
  let pane = $state<HTMLElement | null>(null);
  let paneTop = $state(0);
  let paneH = $state(0);
  /** Not `$state`: read once per tab, and a re-run per wheel tick would buy nothing. */
  let restoredFor: string | null = null;
  $effect(() => {
    // `report` is tracked on purpose: it arrives after mount, and scrolling to row nine hundred
    // of a list that is still empty scrolls to the top.
    const s = scope;
    if (!pane || !report || restoredFor === s) return;
    restoredFor = s;
    pane.scrollTop = store.scrollMemory.get(s)?.top ?? 0;
    paneTop = pane.scrollTop;
    measure();
  });
  function onScroll() {
    if (!pane) return;
    paneTop = pane.scrollTop;
    measure();
    store.scrollMemory.set(scope, { anchor: null, delta: 0, top: paneTop });
  }

  // ---- every patched method ----
  //
  // Three and a half thousand rows is more than a browser will lay out and stay smooth, which is
  // what the four hundred row cut was really about. Rows here are a fixed height, so the honest
  // fix is the same one the mod list uses: keep the full height, draw the screenful.
  const MROW = 33; // 32px row, 1px gap
  const OVERSCAN = 10;
  let vrows = $state<HTMLElement | null>(null);
  /** Where the rows begin inside the scroller: there is a card, a heading and a column header
   *  above them, and their combined height is not worth hardcoding. */
  let vtop = $state(0);
  function measure() {
    if (!pane || !vrows) return;
    vtop = vrows.getBoundingClientRect().top - pane.getBoundingClientRect().top + pane.scrollTop;
  }

  // Every method, contested ones included: the tab says every, and an index a user has to
  // remember excludes a hundred and twenty-five rows is not an index. The contested tab is the
  // same methods asked about differently, not a slice taken out of this one.
  const shown = $derived.by(() => {
    const all = report?.targets ?? [];
    const q = store.patchesQuery.trim().toLowerCase();
    if (!q) return all;
    return all.filter((g) => g.target.toLowerCase().includes(q) || g.patchers.some((p) => p.modName.toLowerCase().includes(q)));
  });
  const window_ = $derived.by(() => {
    if (tab !== "methods") return { from: 0, to: 0 };
    const from = Math.max(0, Math.floor((paneTop - vtop) / MROW) - OVERSCAN);
    const to = Math.min(shown.length, Math.max(from, Math.ceil((paneTop - vtop + paneH) / MROW) + OVERSCAN));
    return { from, to };
  });
  const drawn = $derived(shown.slice(window_.from, window_.to).map((g, i) => ({ g, y: (window_.from + i) * MROW })));
  $effect(() => {
    // Re-measure when the thing being measured moves: a filter empties the list, a tab swaps the
    // card out from under it.
    void shown.length;
    void tab;
    measure();
  });

  // ---- the per-mod table ----
  const sortBy = $derived(store.patchesSort.by);
  const desc = $derived(store.patchesSort.desc);
  function sort(c: PatchesCol) {
    store.patchesSort = sortBy === c ? { by: c, desc: !desc } : { by: c, desc: c !== "name" };
  }
  const rows = $derived.by(() => {
    const list = [...(report?.perMod ?? [])];
    const key = (m: ModPatches) => (sortBy === "name" ? 0 : (m[sortBy] as number));
    list.sort((a, b) => (sortBy === "name" ? a.name.localeCompare(b.name) : key(b) - key(a) || a.name.localeCompare(b.name)));
    return desc ? list : list.reverse();
  });

  // One mod's own methods, fetched when a row opens or the manual section needs them.
  let details = $state<Record<string, ModPatchDetail | null>>({});
  const open = $derived(store.patchesOpen);
  async function load(uid: string) {
    if (uid in details) return;
    details[uid] = null;
    details[uid] = await store.patchesForMod(uid);
  }
  function toggle(uid: string) {
    store.patchesOpen = open === uid ? null : uid;
    if (store.patchesOpen) load(uid);
  }
  $effect(() => {
    // One request per mod, so only when the tab that needs them is the one on screen.
    if (tab !== "manual") return;
    for (const m of manualMods) load(m.uid);
  });
  $effect(() => {
    if (open) load(open);
  });

  const KINDS = new Set(["prefix", "postfix", "transpiler", "finalizer", "reverse", "patch", "unpatch"]);
  /** Not a module-level table: one built at import time would freeze whichever locale happened to
   *  be current when this file first loaded. */
  const kindLabel = (k: string) => (KINDS.has(k) ? t(`patches.kind.${k}`) : k);

  const pct = $derived(job && job.total ? Math.round((job.done / job.total) * 100) : 0);
  const status = $derived(job?.running ? job.phase : job?.summary ? t("patches.status.took", { n: job.summary.seconds }) : t("patches.status.unread"));
  /** Mods fighting over a method, worst first: the ones that rewrite or run before it. */
  const fighters = (g: TargetGroup) => [...g.patchers].sort((a, b) => Number(b.kind === "prefix" || b.kind === "transpiler") - Number(a.kind === "prefix" || a.kind === "transpiler") || (b.priority ?? 400) - (a.priority ?? 400));
</script>

<main class="center">
  <header class="top">
    <h2>{t("patches.title")} <span class="aside">{status}</span></h2>
    {#if job?.running}
      <div class="prog">
        <div class="pl">
          <b class="num">{job.phase === "scanning" ? t("patches.reading", { done: job.done.toLocaleString(), total: job.total.toLocaleString() }) : t("patches.finding")}</b>
          <div class="bar"><i style="width: {job.phase === 'scanning' ? pct : 3}%"></i></div>
        </div>
        <button class="btn" onclick={() => store.stopPatches()}>{t("patches.stop")}</button>
      </div>
    {:else}
      <button class="btn primary" onclick={() => store.scanPatches()}>{@html I.analyze}{report ? t("patches.reread") : t("patches.read")}</button>
    {/if}
  </header>

  <div class="seg tabs" role="tablist">
    {#each tabs as x}
      <button role="tab" class:on={tab === x.id} aria-selected={tab === x.id} onclick={() => (store.patchesTab = x.id)}>
        {x.label}{#if x.count !== null}<span class="num">{x.count.toLocaleString()}</span>{/if}
      </button>
    {/each}
  </div>

  {#key tab}
    <div class="pane" bind:this={pane} bind:clientHeight={paneH} onscroll={onScroll}>
      {#if tab === "overview"}
        <div class="tiles">
          <section class="card tile">
            <div class="k">{t("patches.tile.contested")}</div>
            <div class="v num" class:att={contested.length > 0}>{contested.length.toLocaleString()}</div>
            <div class="sub">{report ? (contested.length ? t("patches.tile.contested.some") : t("patches.tile.contested.none")) : t("patches.status.unread")}</div>
          </section>
          <section class="card tile">
            <div class="k">{t("patches.tile.methods")}</div>
            <div class="v num">{(report?.targets.length ?? 0).toLocaleString()}</div>
            <div class="sub">{t("patches.tile.methods.sub")}</div>
          </section>
          <section class="card tile">
            <div class="k">{t("patches.tile.mods")}</div>
            <div class="v num">{(report?.perMod.length ?? 0).toLocaleString()}</div>
            <div class="sub">{t("patches.tile.mods.sub", { n: store.active.length })}</div>
          </section>
          <section class="card tile">
            <div class="k">{t("patches.tile.patches")}</div>
            <div class="v num">{(report?.perMod ?? []).reduce((n, m) => n + m.patches, 0).toLocaleString()}</div>
            <div class="sub">{t("patches.tile.patches.sub", { n: manualCount.toLocaleString() })}</div>
          </section>
        </div>

        <section class="card">
          <h3>{t("patches.about.title")}</h3>
          <p class="lead">{t("patches.about.lead")}</p>
          {#if job?.summary}
            <p class="report">
              {job.cancelled
                ? t("patches.lastrun.stopped", { n: job.summary.assemblies.toLocaleString(), mods: job.summary.mods, seconds: job.summary.seconds })
                : t("patches.lastrun", { n: job.summary.assemblies.toLocaleString(), mods: job.summary.mods, seconds: job.summary.seconds })}
            </p>
          {/if}
          {#if job?.error}<p class="err">{job.error}</p>{/if}
        </section>
      {:else if tab === "contested"}
        <section class="card contested">
          <p class="lead">{@html t("patches.contested.lead")}</p>
          {#if !contested.length}
            <p class="hint">{t("patches.contested.none")}</p>
          {:else}
            <div class="fights">
              {#each contested as x (x.target)}
                {@const [type, method] = splitTarget(x.target)}
                <div class="fight">
                  <div class="mth"><span class="ty mono">{type}</span><b class="mono">{method}</b></div>
                  <div class="who">
                    {#each fighters(x) as p}
                      <button class="pat {p.kind}" title="{p.modName}: {p.declaringType}.{p.method}" onclick={() => store.reveal(p.uid)}>
                        <span class="kd">{kindLabel(p.kind)}</span>
                        <span class="nm">{p.modName}</span>
                        {#if p.priority != null}<span class="pri num" title={t("patches.contested.priority")}>{p.priority}</span>{/if}
                      </button>
                    {/each}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </section>
      {:else if tab === "methods"}
        <section class="card list">
          <div class="stick">
            <h3>
              <span class="aside">{t("patches.methods.shown", { n: shown.length.toLocaleString() })}</span>
              <span class="tools"><input class="input sm" placeholder={t("patches.methods.filter")} bind:value={store.patchesQuery} /></span>
            </h3>
            <div class="hdr"><span>{t("patches.methods.col.method")}</span><span class="r">{t("patches.methods.col.mods")}</span><span>{t("patches.methods.col.by")}</span></div>
          </div>
          <div class="vrows" bind:this={vrows} style="height: {shown.length * MROW}px">
            {#each drawn as d (d.g.target)}
              {@const [type, method] = splitTarget(d.g.target)}
              <div class="row" style="top: {d.y}px">
                <span class="mth2 mono" title={d.g.target}><span class="ty">{type}::</span>{method}</span>
                <span class="r num">{d.g.patchers.length}</span>
                <span class="by">{#each d.g.patchers as p, i}<button class="lnk" onclick={() => store.reveal(p.uid)}>{p.modName}</button><span class="kd2">{kindLabel(p.kind)}</span>{i < d.g.patchers.length - 1 ? " · " : ""}{/each}</span>
              </div>
            {/each}
          </div>
          {#if !shown.length}<div class="hint">{store.patchesQuery ? t("patches.methods.nomatch") : t("patches.methods.none")}</div>{/if}
        </section>
      {:else if tab === "permod"}
        <section class="card permod">
          <p class="lead">{t("patches.permod.lead")}</p>
          <div class="mhdr stick">
            <button class="h" class:on={sortBy === "name"} onclick={() => sort("name")}>{t("patches.permod.col.mod")}</button>
            <button class="h r" class:on={sortBy === "patches"} onclick={() => sort("patches")}>{t("patches.permod.col.patches")}</button>
            <button class="h r" class:on={sortBy === "prefixes"} onclick={() => sort("prefixes")}>{t("patches.permod.col.before")}</button>
            <button class="h r" class:on={sortBy === "postfixes"} onclick={() => sort("postfixes")}>{t("patches.permod.col.after")}</button>
            <button class="h r" class:on={sortBy === "transpilers"} onclick={() => sort("transpilers")}>{t("patches.permod.col.rewrites")}</button>
            <button class="h r" class:on={sortBy === "manual"} onclick={() => sort("manual")}>{t("patches.permod.col.unreadable")}</button>
            <span class="h">{t("patches.permod.col.harmonyid")}</span>
          </div>
          <div class="rows">
            {#each rows as m (m.uid)}
              <button class="mrow" class:open={open === m.uid} onclick={() => toggle(m.uid)}>
                <span class="nm"><b>{m.name}</b></span>
                <span class="r num">{m.patches.toLocaleString()}</span>
                <span class="r num">{m.prefixes || ""}</span>
                <span class="r num">{m.postfixes || ""}</span>
                <span class="r num">{m.transpilers || ""}</span>
                <span class="r num" class:warn={m.manual > 0}>{m.manual || ""}</span>
                <span class="hid mono">{m.harmonyIds.join(", ") || t("patches.permod.noid")}</span>
              </button>
              {#if open === m.uid}
                <div class="detail">
                  {#if details[m.uid] === null}
                    <span class="hint">{t("patches.permod.reading")}</span>
                  {:else if details[m.uid]}
                    {@const d = details[m.uid]!}
                    {#if d.targets.length}
                      <div class="dlist">
                        {#each d.targets as p}
                          {@const [type, method] = splitTarget(patchTargetName(p))}
                          <div class="drow"><span class="kd3 {p.kind}">{kindLabel(p.kind)}</span><span class="mono"><span class="ty">{type}::</span>{method}</span><span class="from mono" title="{p.declaringType}.{p.method}">{p.declaringType.split(".").pop()}.{p.method}</span></div>
                        {/each}
                      </div>
                    {:else}
                      <span class="hint">{t("patches.permod.unnamed")}</span>
                    {/if}
                    {#if d.contested.length}
                      <div class="hint">{t("patches.permod.contested", { n: d.contested.length, names: d.contested.map((x) => splitTarget(x.target)[1]).join(", ") })}</div>
                    {/if}
                  {/if}
                </div>
              {/if}
            {/each}
            {#if !rows.length}<div class="hint">{t("patches.permod.none")}</div>{/if}
          </div>
        </section>
      {:else}
        <section class="card manual">
          <p class="lead">{t("patches.manual.lead")}</p>
          {#if !manualMods.length}
            <p class="hint">{t("patches.manual.none")}</p>
          {:else}
            <div class="mans">
              {#each manualMods as m (m.uid)}
                <div class="man">
                  <button class="lnk b" onclick={() => store.reveal(m.uid)}>{m.name}</button>
                  <div class="mlist">
                    {#each details[m.uid]?.manual ?? [] as x}
                      <div class="mline"><span class="mono">{x.declaringType}.{x.method}</span><span class="why">{x.detail}</span></div>
                    {:else}
                      <div class="mline"><span class="why">{t("patches.manual.places", { n: m.manual })}</span></div>
                    {/each}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
          {#if unreadable.length}
            <details class="errs" open>
              <summary>{t("patches.manual.unreadable", { n: unreadable.reduce((n, m) => n + m.unreadable.length, 0) })}</summary>
              <div class="errlist">{#each unreadable as m}{#each m.unreadable as u}<div><b>{m.name}</b> <span class="mono">{u}</span></div>{/each}{/each}</div>
            </details>
          {/if}
        </section>
      {/if}
    </div>
  {/key}
</main>

<style>
  /* The frame does not scroll: the title, the read button and the tab bar stay put, and the tab
     underneath them is the only thing that moves. */
  .center { display: flex; flex-direction: column; gap: 10px; min-height: 0; min-width: 0; overflow: hidden; }
  .pane { flex: 1; min-height: 0; overflow: hidden auto; display: flex; flex-direction: column; gap: 12px; padding-bottom: 14px; }

  .top { display: flex; align-items: center; gap: 16px; min-width: 0; }
  .top h2 { font-size: 15px; font-weight: 700; margin: 0; display: flex; align-items: baseline; gap: 10px; min-width: 0; }
  .top .aside { font-size: 12px; font-weight: 500; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .top > .btn { margin-left: auto; flex: none; }
  .prog { margin-left: auto; display: flex; align-items: center; gap: 12px; min-width: 0; flex: 0 1 420px; }
  .pl { display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--text-2); min-width: 0; flex: 1; }
  .pl b { font-size: 12px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .bar { height: 6px; border-radius: 3px; background: var(--surface-3); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--amber); border-radius: 3px; transition: width 0.3s; }

  .tabs { align-self: flex-start; max-width: 100%; flex-wrap: wrap; }

  .tiles { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; }
  .tile .k { font-size: 12px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-3); }
  .tile .v { font-size: 26px; font-weight: 800; letter-spacing: -0.02em; margin-top: 4px; }
  .tile .v.att { color: var(--amber); }
  .tile .sub { font-size: 12px; color: var(--text-3); margin-top: 2px; }
  .lead { margin: 0 0 12px; color: var(--text-2); font-size: 13px; line-height: 1.5; max-width: 96ch; }
  .lead :global(b) { color: var(--text); font-weight: 600; }
  .hint { color: var(--text-3); font-size: 12px; line-height: 1.45; margin: 6px 0 0; }
  .err { color: var(--red); font-size: 12.5px; line-height: 1.5; margin: 8px 0 0; max-width: 90ch; }
  .report { font-size: 12.5px; color: var(--text-2); margin: 0; }

  /* contested */
  .fights { display: flex; flex-direction: column; gap: 3px; }
  .fight { display: grid; grid-template-columns: minmax(0, 5fr) minmax(0, 7fr); gap: 12px; align-items: center; padding: 7px 8px; border-radius: 9px; background: var(--surface-2); }
  .mth { min-width: 0; display: flex; flex-direction: column; }
  .mth .ty { font-size: 11px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mth b { font-size: 13px; color: var(--text); font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .who { display: flex; flex-wrap: wrap; gap: 5px; min-width: 0; }
  .pat { display: inline-flex; align-items: center; gap: 6px; max-width: 100%; padding: 3px 8px; border-radius: 7px; background: var(--surface-3); font-size: 12px; color: var(--text-2); }
  .pat:hover { background: var(--surface-4); color: var(--text); }
  .pat .kd { font-size: 10px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--text-3); }
  .pat.prefix .kd { color: var(--amber); }
  .pat.transpiler .kd { color: var(--red); }
  .pat .nm { font-weight: 600; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pat .pri { font-size: 11px; color: var(--text-3); }

  /* every patched method */
  /* The filter and the column names follow the list down it: four hundred rows in, a column of
     numbers with no heading over it is a column of numbers. The negative margin pulls the strip
     out to the card's edges so nothing scrolls through the padding beside it. */
  .stick { position: sticky; top: 0; z-index: 2; background: var(--surface); margin: -14px -14px 0; padding: 14px 14px 0; }
  .list h3 { display: flex; align-items: center; gap: 10px; }
  .tools { margin-left: auto; display: flex; gap: 12px; align-items: center; font-weight: 500; text-transform: none; letter-spacing: 0; }
  .input.sm { height: 28px; font-size: 12.5px; width: 220px; }
  .hdr, .row { display: grid; grid-template-columns: minmax(0, 1fr) 60px minmax(0, 1fr); gap: 12px; align-items: center; padding: 0 8px; }
  .hdr { font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-3); height: 28px; }
  .rows { display: flex; flex-direction: column; gap: 1px; }
  /* The list keeps its true height and draws the screenful: three thousand rows of markup is a
     locked window, and cutting the list at four hundred was the workaround. */
  .vrows { position: relative; }
  .vrows .row { position: absolute; left: 0; right: 0; }
  .row { height: 32px; border-radius: 9px; font-size: 12.5px; }
  .row:hover { background: var(--surface-2); }
  .mth2 { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); }
  .ty { color: var(--text-3); }
  .r { text-align: right; color: var(--text-2); }
  .by { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-3); font-size: 12px; }
  .lnk { color: var(--blue); font-weight: 600; }
  .lnk:hover { text-decoration: underline; }
  .kd2 { font-size: 10.5px; color: var(--text-4); margin-left: 5px; }

  /* per mod */
  .mhdr, .mrow { display: grid; grid-template-columns: minmax(0, 1fr) 72px 66px 60px 78px 88px minmax(0, 200px); gap: 10px; align-items: center; padding: 0 8px; text-align: left; }
  .mhdr { height: 28px; }
  /* `.mhdr` sets its own padding, so the sticky strip's has to be restated here or the column
     names sit fourteen pixels left of the rows they name. 22px is the card's 14 plus the row's 8. */
  .mhdr.stick { height: 42px; margin: -14px -14px 0; padding: 14px 22px 0; }
  .mhdr .h { font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-3); text-align: left; }
  .mhdr .h.r { text-align: right; }
  .mhdr button.h:hover { color: var(--text-2); }
  .mhdr .h.on { color: var(--amber); }
  .mrow { height: 34px; border-radius: 9px; font-size: 12.5px; width: 100%; }
  .mrow:hover { background: var(--surface-2); }
  .mrow.open { background: var(--surface-3); }
  .mrow .nm { min-width: 0; display: flex; align-items: center; gap: 8px; }
  .mrow .nm b { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mrow .r.warn { color: var(--amber); font-weight: 700; }
  .hid { min-width: 0; color: var(--text-3); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* No height cap: the pane scrolls, and a mod with four hundred patches has four hundred here. */
  .detail { padding: 6px 8px 12px 20px; }
  .dlist { display: flex; flex-direction: column; gap: 2px; }
  .drow { display: grid; grid-template-columns: 80px minmax(0, 1fr) minmax(0, 220px); gap: 10px; align-items: center; font-size: 12px; height: 22px; }
  .drow .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); }
  .drow .from { color: var(--text-4); font-size: 11px; text-align: right; }
  .kd3 { font-size: 10px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--text-3); }
  .kd3.prefix { color: var(--amber); }
  .kd3.transpiler { color: var(--red); }
  .kd3.postfix { color: var(--green); }

  /* unreadable */
  .mans { display: flex; flex-direction: column; gap: 8px; }
  .man { display: grid; grid-template-columns: minmax(0, 220px) minmax(0, 1fr); gap: 12px; align-items: start; padding: 6px 8px; border-radius: 9px; background: var(--surface-2); }
  .man .b { text-align: left; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mlist { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .mline { display: flex; flex-wrap: wrap; gap: 4px 10px; font-size: 12px; min-width: 0; }
  .mline .mono { color: var(--text-2); overflow-wrap: anywhere; }
  .mline .why { color: var(--text-3); }
  .errs { margin-top: 12px; font-size: 12.5px; }
  .errs summary { cursor: pointer; font-weight: 600; color: var(--amber); }
  .errlist { margin-top: 6px; display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--text-3); }
  .errlist b { color: var(--text-2); }
  @media (max-width: 1240px) {
    .tiles { grid-template-columns: 1fr 1fr; }
    .fight { grid-template-columns: minmax(0, 1fr); gap: 6px; }
    .mhdr, .mrow { grid-template-columns: minmax(0, 1fr) 72px 66px 60px 78px 88px; }
    .mhdr .h:last-child, .mrow .hid { display: none; }
  }
</style>
