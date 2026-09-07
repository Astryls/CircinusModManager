<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { patchTargetName, splitTarget, type ModPatchDetail, type ModPatches, type TargetGroup } from "$lib/types";

  const job = $derived(store.patchJob);
  const report = $derived(store.patchReport);
  store.refreshPatches();

  const contested = $derived((report?.targets ?? []).filter((t) => t.contested));
  const rest = $derived((report?.targets ?? []).filter((t) => !t.contested));
  const manualMods = $derived((report?.perMod ?? []).filter((m) => m.manual > 0));
  const unreadable = $derived((report?.perMod ?? []).filter((m) => m.unreadable.length));

  let query = $state("");
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rest;
    return rest.filter((t) => t.target.toLowerCase().includes(q) || t.patchers.some((p) => p.modName.toLowerCase().includes(q)));
  });

  // ---- the per-mod table ----
  type Col = "name" | "patches" | "prefixes" | "postfixes" | "transpilers" | "manual";
  let sortBy = $state<Col>("patches");
  let desc = $state(true);
  function sort(c: Col) {
    if (sortBy === c) desc = !desc;
    else { sortBy = c; desc = c !== "name"; }
  }
  const rows = $derived.by(() => {
    const list = [...(report?.perMod ?? [])];
    const key = (m: ModPatches) => (sortBy === "name" ? 0 : (m[sortBy] as number));
    list.sort((a, b) => (sortBy === "name" ? a.name.localeCompare(b.name) : key(b) - key(a) || a.name.localeCompare(b.name)));
    return desc ? list : list.reverse();
  });

  // One mod's own methods, fetched when a row opens or the manual section needs them.
  let details = $state<Record<string, ModPatchDetail | null>>({});
  let open = $state<string | null>(null);
  async function load(uid: string) {
    if (uid in details) return;
    details[uid] = null;
    details[uid] = await store.patchesForMod(uid);
  }
  function toggle(uid: string) {
    open = open === uid ? null : uid;
    if (open) load(uid);
  }
  $effect(() => {
    for (const m of manualMods) load(m.uid);
  });

  const kindLabel: Record<string, string> = { prefix: "runs before", postfix: "runs after", transpiler: "rewrites", finalizer: "catches errors in", reverse: "copies", patch: "patches", unpatch: "unpatches" };
  const pct = $derived(job && job.total ? Math.round((job.done / job.total) * 100) : 0);
  /** Mods fighting over a method, worst first: the ones that rewrite or run before it. */
  const fighters = (t: TargetGroup) => [...t.patchers].sort((a, b) => Number(b.kind === "prefix" || b.kind === "transpiler") - Number(a.kind === "prefix" || a.kind === "transpiler") || (b.priority ?? 400) - (a.priority ?? 400));
</script>

<main class="center">
    <div class="tiles">
      <section class="card tile">
        <div class="k">Contested methods</div>
        <div class="v num" class:att={contested.length > 0}>{contested.length.toLocaleString()}</div>
        <div class="sub">{contested.length ? "two or more mods change the same method" : report ? "nothing two mods fight over" : "not read yet"}</div>
      </section>
      <section class="card tile">
        <div class="k">Patched methods</div>
        <div class="v num">{(report?.targets.length ?? 0).toLocaleString()}</div>
        <div class="sub">game methods your active mods attach to</div>
      </section>
      <section class="card tile">
        <div class="k">Mods with code</div>
        <div class="v num">{(report?.perMod.length ?? 0).toLocaleString()}</div>
        <div class="sub">of {store.active.length} active</div>
      </section>
      <section class="card tile">
        <div class="k">Patches in all</div>
        <div class="v num">{(report?.perMod ?? []).reduce((n, m) => n + m.patches, 0).toLocaleString()}</div>
        <div class="sub">{(report?.perMod ?? []).reduce((n, m) => n + m.manual, 0).toLocaleString()} more Circinus cannot follow</div>
      </section>
    </div>

    <section class="card run">
      <h3>What your mods patch <span class="aside">{job?.running ? job.phase : job?.summary ? `read in ${job.summary.seconds}s` : "not read yet"}</span></h3>
      <p class="lead">
        Circinus reads every active mod's assemblies itself and reports the game methods they attach to. Nothing is loaded or run: it reads the metadata and instructions of a file the way a disassembler does, so a mod's code never executes. Results are kept until a mod changes, so a second run is quick.
      </p>
      {#if job?.running}
        <div class="prog">
          <div class="bar"><i style="width: {job.phase === 'scanning' ? pct : 3}%"></i></div>
          <div class="pl">
            <b class="num">{job.phase === "scanning" ? `Reading ${job.done.toLocaleString()} of ${job.total.toLocaleString()} assemblies` : "Finding assemblies…"}</b>
            <span class="mono cur">{job.current}</span>
          </div>
          <button class="btn" onclick={() => store.stopPatches()}>Stop</button>
        </div>
      {:else}
        <div class="acts">
          <button class="btn primary" onclick={() => store.scanPatches()}>{@html I.analyze}{report ? "Read them again" : "Read the assemblies"}</button>
          {#if job?.summary}<span class="report">Last run read <b class="num">{job.summary.assemblies.toLocaleString()}</b> assemblies in {job.summary.mods} mods{job.cancelled ? ", then stopped" : ""} in {job.summary.seconds}s.</span>{/if}
        </div>
        {#if job?.error}<p class="err">{job.error}</p>{/if}
      {/if}
    </section>

    {#if report}
      <section class="card contested">
        <h3>Two mods over one method <span class="aside">{contested.length}</span></h3>
        <p class="lead">A method is contested when two mods <b>run before</b> it or <b>rewrite</b> it: only one of them decides what the original does, so one usually loses. Mods that only <b>run after</b> a method stack cleanly and are not counted here.</p>
        {#if !contested.length}
          <p class="hint">Nothing here. Every method your active mods patch is either patched by one mod, or only added to.</p>
        {:else}
          <div class="fights">
            {#each contested.slice(0, 120) as t (t.target)}
              {@const [type, method] = splitTarget(t.target)}
              <div class="fight">
                <div class="mth"><span class="ty mono">{type}</span><b class="mono">{method}</b></div>
                <div class="who">
                  {#each fighters(t) as p}
                    <button class="pat {p.kind}" title="{p.modName}: {p.declaringType}.{p.method}" onclick={() => store.reveal(p.uid)}>
                      <span class="kd">{kindLabel[p.kind] ?? p.kind}</span>
                      <span class="nm">{p.modName}</span>
                      {#if p.priority != null}<span class="pri num" title="Harmony priority; higher goes first">{p.priority}</span>{/if}
                    </button>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
          {#if contested.length > 120}<p class="hint">…{contested.length - 120} more contested methods; the per-mod table below has them all.</p>{/if}
        {/if}
      </section>

      <section class="card list">
        <h3>Every patched method <span class="aside">{shown.length.toLocaleString()} shown</span>
          <span class="tools"><input class="input sm" placeholder="Filter by method or mod" bind:value={query} /></span>
        </h3>
        <div class="hdr"><span>Method</span><span class="r">Mods</span><span>Patched by</span></div>
        <div class="rows">
          {#each shown.slice(0, 400) as t (t.target)}
            {@const [type, method] = splitTarget(t.target)}
            <div class="row">
              <span class="mth2 mono" title={t.target}><span class="ty">{type}::</span>{method}</span>
              <span class="r num">{t.patchers.length}</span>
              <span class="by">{#each t.patchers as p, i}<button class="lnk" onclick={() => store.reveal(p.uid)}>{p.modName}</button><span class="kd2">{kindLabel[p.kind] ?? p.kind}</span>{i < t.patchers.length - 1 ? " · " : ""}{/each}</span>
            </div>
          {/each}
          {#if shown.length > 400}<div class="hint">…{shown.length - 400} more; narrow the filter.</div>{/if}
          {#if !shown.length}<div class="hint">{query ? "No method or mod matches that." : "Nothing patched: no active mod ships code."}</div>{/if}
        </div>
      </section>

      <section class="card permod">
        <h3>Per mod <span class="aside">{rows.length}</span></h3>
        <p class="lead">Click a mod to see the methods it patches. A Harmony id is the name a mod gives its own patches; the game's logs use it to say whose patch threw.</p>
        <div class="mhdr">
          <button class="h" class:on={sortBy === "name"} onclick={() => sort("name")}>Mod</button>
          <button class="h r" class:on={sortBy === "patches"} onclick={() => sort("patches")}>Patches</button>
          <button class="h r" class:on={sortBy === "prefixes"} onclick={() => sort("prefixes")}>Before</button>
          <button class="h r" class:on={sortBy === "postfixes"} onclick={() => sort("postfixes")}>After</button>
          <button class="h r" class:on={sortBy === "transpilers"} onclick={() => sort("transpilers")}>Rewrites</button>
          <button class="h r" class:on={sortBy === "manual"} onclick={() => sort("manual")}>Unreadable</button>
          <span class="h">Harmony id</span>
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
              <span class="hid mono">{m.harmonyIds.join(", ") || "none found"}</span>
            </button>
            {#if open === m.uid}
              <div class="detail">
                {#if details[m.uid] === null}
                  <span class="hint">Reading…</span>
                {:else if details[m.uid]}
                  {@const d = details[m.uid]!}
                  {#if d.targets.length}
                    <div class="dlist">
                      {#each d.targets as p}
                        {@const [type, method] = splitTarget(patchTargetName(p))}
                        <div class="drow"><span class="kd3 {p.kind}">{kindLabel[p.kind] ?? p.kind}</span><span class="mono"><span class="ty">{type}::</span>{method}</span><span class="from mono" title="{p.declaringType}.{p.method}">{p.declaringType.split(".").pop()}.{p.method}</span></div>
                      {/each}
                    </div>
                  {:else}
                    <span class="hint">No patch this tool can name: everything it does happens at runtime.</span>
                  {/if}
                  {#if d.contested.length}<div class="hint">In {d.contested.length} contested method{d.contested.length === 1 ? "" : "s"}: {d.contested.slice(0, 3).map((t) => splitTarget(t.target)[1]).join(", ")}{d.contested.length > 3 ? "…" : ""}</div>{/if}
                {/if}
              </div>
            {/if}
          {/each}
          {#if !rows.length}<div class="hint">No active mod ships an assembly.</div>{/if}
        </div>
      </section>

      <section class="card manual">
        <h3>Patched in ways we cannot follow <span class="aside">{manualMods.reduce((n, m) => n + m.manual, 0)}</span></h3>
        <p class="lead">A mod can work out at runtime which method to patch — from a setting, a name it builds, or the mods it finds installed. A target computed like that is not written down in the file, so reading it cannot say what it will be. These are the places that do it, so you know where to look when something goes wrong there.</p>
        {#if !manualMods.length}
          <p class="hint">Every patch your active mods declare names its target outright.</p>
        {:else}
          <div class="mans">
            {#each manualMods as m (m.uid)}
              <div class="man">
                <button class="lnk b" onclick={() => store.reveal(m.uid)}>{m.name}</button>
                <div class="mlist">
                  {#each details[m.uid]?.manual ?? [] as x}
                    <div class="mline"><span class="mono">{x.declaringType}.{x.method}</span><span class="why">{x.detail}</span></div>
                  {:else}
                    <div class="mline"><span class="why">{m.manual} place{m.manual === 1 ? "" : "s"}</span></div>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        {/if}
        {#if unreadable.length}
          <details class="errs">
            <summary>{unreadable.reduce((n, m) => n + m.unreadable.length, 0)} assemblies could not be read at all</summary>
            <div class="errlist">{#each unreadable as m}{#each m.unreadable as u}<div><b>{m.name}</b> <span class="mono">{u}</span></div>{/each}{/each}</div>
          </details>
        {/if}
      </section>
    {/if}
</main>

<style>
  .center { display: flex; flex-direction: column; gap: 12px; min-height: 0; min-width: 0; overflow: hidden auto; padding-bottom: 14px; }
  .tiles { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; }
  .tile .k { font-size: 12px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-3); }
  .tile .v { font-size: 26px; font-weight: 800; letter-spacing: -0.02em; margin-top: 4px; }
  .tile .v.att { color: var(--amber); }
  .tile .sub { font-size: 12px; color: var(--text-3); margin-top: 2px; }
  .lead { margin: 0 0 12px; color: var(--text-2); font-size: 13px; line-height: 1.5; max-width: 96ch; }
  .lead b { color: var(--text); font-weight: 600; }
  .hint { color: var(--text-3); font-size: 12px; line-height: 1.45; margin: 6px 0 0; }
  .err { color: var(--red); font-size: 12.5px; line-height: 1.5; margin: 8px 0 0; max-width: 90ch; }
  .acts { display: flex; gap: 12px; flex-wrap: wrap; align-items: center; }
  .report { font-size: 12.5px; color: var(--text-2); }
  .prog { display: grid; grid-template-columns: 1fr auto; gap: 8px 12px; align-items: center; }
  .bar { grid-column: 1 / -1; height: 8px; border-radius: 4px; background: var(--surface-3); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--amber); border-radius: 4px; transition: width 0.3s; }
  .pl { display: flex; flex-direction: column; gap: 2px; font-size: 12.5px; color: var(--text-2); min-width: 0; }
  .pl b { font-size: 14px; color: var(--text); }
  .cur { color: var(--text-3); font-size: 11.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

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
  .list h3, .permod h3 { display: flex; align-items: center; gap: 10px; }
  .tools { margin-left: auto; display: flex; gap: 12px; align-items: center; font-weight: 500; text-transform: none; letter-spacing: 0; }
  .input.sm { height: 28px; font-size: 12.5px; width: 220px; }
  .hdr, .row { display: grid; grid-template-columns: minmax(0, 1fr) 60px minmax(0, 1fr); gap: 12px; align-items: center; padding: 0 8px; }
  .hdr { font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-3); height: 28px; }
  .rows { display: flex; flex-direction: column; gap: 1px; }
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
  .detail { padding: 6px 8px 12px 20px; max-height: 300px; overflow: hidden auto; }
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
  .errlist { margin-top: 6px; max-height: 180px; overflow: hidden auto; display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--text-3); }
  .errlist b { color: var(--text-2); }
  @media (max-width: 1240px) {
    .tiles { grid-template-columns: 1fr 1fr; }
    .fight { grid-template-columns: minmax(0, 1fr); gap: 6px; }
    .mhdr, .mrow { grid-template-columns: minmax(0, 1fr) 72px 66px 60px 78px 88px; }
    .mhdr .h:last-child, .mrow .hid { display: none; }
  }
</style>
