<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { formatBytes, type DdsFinding, type DdsFormat, type DdsQuality, type ModTextures } from "$lib/types";

  const t = $derived(store.tex);
  const s = $derived(store.snap?.settings.dds);
  const totals = $derived(store.ddsTotals);
  const rows = $derived(store.texOverview);
  store.refreshTextures();
  let query = $state("");
  let onlyActive = $state(true);
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return rows.filter((r) => (!onlyActive || r.active) && (!q || r.name.toLowerCase().includes(q)));
  });
  const activeUids = $derived(rows.filter((r) => r.active && !r.excluded && r.pngs > 0).map((r) => r.uid));
  const allUids = $derived(rows.filter((r) => !r.excluded && r.pngs > 0).map((r) => r.uid));
  const convertedUids = $derived(rows.filter((r) => r.converted > 0).map((r) => r.uid));
  const pngsActive = $derived(rows.filter((r) => r.active).reduce((n, r) => n + r.pngs, 0));
  const pngsAll = $derived(rows.reduce((n, r) => n + r.pngs, 0));
  const pct = $derived(t && t.progress.total ? Math.round((t.progress.done / t.progress.total) * 100) : 0);
  let now = $state(Math.floor(Date.now() / 1000));
  $effect(() => {
    const i = setInterval(() => (now = Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(i);
  });
  const rate = $derived.by(() => {
    if (!t?.running || !t.startedAt) return null;
    const secs = Math.max(1, now - t.startedAt);
    const per = t.progress.done / secs;
    const left = per > 0 ? Math.round((t.progress.total - t.progress.done) / per) : null;
    return { per, left };
  });
  const eta = (secs: number | null) => (secs == null ? "" : secs < 90 ? `${secs}s` : secs < 5400 ? `${Math.round(secs / 60)} min` : `${(secs / 3600).toFixed(1)} h`);
  const audit = $derived(store.audit);
  const auditActive = $derived(rows.filter((r) => r.active).map((r) => r.uid));
  const auditAll = $derived(rows.map((r) => r.uid));
  const fixAll = $derived((audit?.mods ?? []).filter((m) => m.findings.some((f) => f.fixable)).map((m) => [m.uid, []] as [string, string[]]));
  const problem = (f: DdsFinding) =>
    f.problem.kind === "notMultipleOf4" ? `${f.width}×${f.height} ${f.format}: a side is not a multiple of 4` : f.problem.kind === "truncated" ? `${f.format}: only ${formatBytes(f.problem.actual)} of ${formatBytes(f.problem.expected)}, the file is cut short` : `cannot be read: ${f.problem.reason}`;
  const phaseLabel = (p: string) => (p === "scanning" ? "Finding textures…" : p === "reverting" ? "Removing DDS files…" : p === "auditing" ? "Reading DDS headers…" : p === "fixing" ? "Rebuilding…" : "");

  function update(patch: Partial<NonNullable<typeof s>>) {
    if (!s) return;
    store.updateSettings({ dds: { ...s, ...patch } });
  }
  function revertOne(r: ModTextures) {
    store.revertTextures([r.uid]);
  }
  const qualities: { id: DdsQuality; label: string; hint: string }[] = [
    { id: "quick", label: "Quick", hint: "Fastest. Fine for a first pass" },
    { id: "balanced", label: "Balanced", hint: "Fast. The default" },
    { id: "high", label: "High", hint: "About half the speed of Balanced" },
    { id: "max", label: "Max", hint: "Slow, about three times slower than High" }
  ];
  const formats: { id: DdsFormat; label: string; hint: string }[] = [
    { id: "bc7", label: "BC7", hint: "Best quality for transparency. What todds and RimSort use" },
    { id: "bc3", label: "BC3 (DXT5)", hint: "Older format, rougher transparency, same size" }
  ];
</script>

<main class="center">
  <div class="tiles">
    <section class="card tile">
      <div class="k">Converted</div>
      <div class="v num">{totals.files.toLocaleString()}</div>
      <div class="sub">textures in {totals.mods} mod{totals.mods === 1 ? "" : "s"}</div>
    </section>
    <section class="card tile">
      <div class="k">On disk</div>
      <div class="v num">{formatBytes(totals.ddsBytes)}</div>
      <div class="sub">of DDS next to {formatBytes(totals.pngBytes)} of PNG, which is kept</div>
    </section>
    <section class="card tile">
      <div class="k">Graphics memory saved</div>
      <div class="v num">{totals.vramBefore ? `${Math.round((1 - totals.ddsBytes / totals.vramBefore) * 100)} %` : "n/a"}</div>
      <div class="sub">{formatBytes(totals.vramBefore)} uncompressed becomes {formatBytes(totals.ddsBytes)}</div>
    </section>
    <section class="card tile">
      <div class="k">PNG textures</div>
      <div class="v num">{pngsActive.toLocaleString()}</div>
      <div class="sub">in active mods · {pngsAll.toLocaleString()} installed</div>
    </section>
  </div>

  <div class="top">
    <section class="card run">
      <h3>Make DDS textures <span class="aside">{t?.running ? t.phase : "idle"}</span></h3>
      <p class="lead">Writes a <span class="mono">.dds</span> file next to every PNG in a mod's <span class="mono">Textures</span> folders. RimWorld then loads the DDS, which is already in the format the graphics card wants, and skips decoding the PNG. PNGs are never changed, so removing the DDS files puts everything back. Every file is decoded and checked before it is put in place, and a mod that updates gets its files checked again.</p>
      {#if t?.running}
        <div class="prog">
          <div class="bar"><i style="width: {t.phase === 'converting' || t.phase === 'fixing' ? pct : 3}%"></i></div>
          <div class="pl">
            <b class="num">{t.phase === "converting" || t.phase === "fixing" ? `${t.progress.done.toLocaleString()} of ${t.progress.total.toLocaleString()}` : phaseLabel(t.phase)}</b>
            {#if t.phase === "converting"}<span>{t.progress.converted} converted{t.progress.failed ? ` · ${t.progress.failed} failed` : ""} · {formatBytes(t.progress.pngBytes)} → {formatBytes(t.progress.ddsBytes)}{rate ? ` · ${rate.per.toFixed(1)}/s · ${eta(rate.left)} left` : ""}</span>{/if}
            <span class="mono cur">{t.progress.current}</span>
          </div>
          <button class="btn" onclick={() => store.cancelTextures()}>Stop</button>
        </div>
      {:else}
        <div class="acts">
          <button class="btn primary" disabled={!activeUids.length} onclick={() => store.optimizeTextures(activeUids)}>{@html I.image}Convert active mods <span class="cnt num">{activeUids.length}</span></button>
          <button class="btn" disabled={!allUids.length} onclick={() => store.optimizeTextures(allUids)}>Everything installed <span class="cnt num">{allUids.length}</span></button>
          <button class="btn" disabled={!convertedUids.length} onclick={() => store.revertTextures(convertedUids)}>Remove every DDS Circinus made</button>
        </div>
        {#if t?.report}
          {@const r = t.report}
          <div class="report">
            {#if r.reverted || r.bytesFreed}
              Last run removed <b class="num">{r.reverted}</b> files{r.restored ? ` (${r.restored} originals put back)` : ""}, freeing {formatBytes(r.bytesFreed)}.
            {:else if r.fixed || (t.phase === "idle" && audit && !r.converted && !r.current)}
              Last run rebuilt <b class="num">{r.fixed ?? 0}</b> file{(r.fixed ?? 0) === 1 ? "" : "s"}{r.failed ? `, ${r.failed} failed` : ""} in {r.seconds}s. The originals are next to them as <span class="mono">.circinus-orig</span>.
            {:else}
              Last run: <b class="num">{r.converted}</b> converted{r.failed ? `, ${r.failed} failed` : ""}{r.current ? `, ${r.current} already current` : ""}{r.shipped ? `, ${r.shipped} left alone because the author ships a DDS` : ""} across {r.mods} mods in {r.seconds}s{r.cancelled ? ", stopped early" : ""}. {formatBytes(r.pngBytes)} of PNG became {formatBytes(r.ddsBytes)} of DDS.
            {/if}
          </div>
        {/if}
      {/if}
      {#if t?.errors.length}
        <details class="errs">
          <summary>{t.errors.length} file{t.errors.length === 1 ? "" : "s"} could not be converted and stay as PNG</summary>
          <div class="errlist">{#each t.errors.slice(0, 200) as [m, rel, msg]}<div><b>{m}</b> <span class="mono">{rel}</span>: {msg}</div>{/each}</div>
        </details>
      {/if}
    </section>

    <section class="card opts">
      <h3>Settings</h3>
      <div class="opt">
        <span class="l">Transparent textures</span>
        <div class="seg">{#each formats as f}<button class:on={s?.alphaFormat === f.id} title={f.hint} onclick={() => update({ alphaFormat: f.id })}>{f.label}</button>{/each}</div>
      </div>
      <div class="opt">
        <span class="l">Quality</span>
        <div class="seg">{#each qualities as q}<button class:on={s?.quality === q.id} title={q.hint} onclick={() => update({ quality: q.id })}>{q.label}</button>{/each}</div>
      </div>
      <label class="switch"><input type="checkbox" checked={s?.mipmaps ?? true} onchange={(e) => update({ mipmaps: e.currentTarget.checked })} />Make mipmaps (smoother when zoomed out, recommended)</label>
      <label class="switch"><input type="checkbox" checked={s?.auto ?? false} onchange={(e) => update({ auto: e.currentTarget.checked })} />Convert new and updated mods automatically</label>
      <div class="opt">
        <span class="l">Threads</span>
        <input class="input num" type="number" min="0" max="64" value={s?.threads ?? 0} onchange={(e) => update({ threads: Math.max(0, Math.min(64, Number(e.currentTarget.value) || 0)) })} />
        <span class="hint">0 = all cores but one</span>
      </div>
      <p class="hint">Changing the format converts textures with transparency again next time. Changing quality only affects new work. Sides that are not multiples of four are resized up, never padded.</p>
    </section>
  </div>

  <section class="card auditc">
    <h3>DDS files the game cannot load <span class="aside">{audit ? `${audit.files} in ${audit.mods.length} of ${audit.modsChecked} mods · ${audit.seconds}s` : "not checked yet"}</span></h3>
    <p class="lead">Some mods ship <span class="mono">.dds</span> files the game cannot load, most often because a side is not a multiple of 4. RimWorld logs "Failed to create texture because of invalid parameters" and the art goes missing. This check reads the header of every DDS file Circinus did not write. Fix rebuilds the file from the PNG next to it (or from its own pixels), resized so it loads, and keeps the original as <span class="mono">.circinus-orig</span>. Revert puts the original back.</p>
    <div class="acts">
      <button class="btn primary" disabled={t?.running || !auditActive.length} onclick={() => store.auditTextures(auditActive)}>{@html I.check}Check active mods <span class="cnt num">{auditActive.length}</span></button>
      <button class="btn" disabled={t?.running || !auditAll.length} onclick={() => store.auditTextures(auditAll)}>Everything installed <span class="cnt num">{auditAll.length}</span></button>
      {#if audit?.fixable}<button class="btn" disabled={t?.running} onclick={() => store.fixTextures(fixAll)}>Fix all <span class="cnt num">{audit.fixable}</span></button>{/if}
    </div>
    {#if audit}
      {#if !audit.mods.length}
        <p class="hint">Every DDS file in {audit.modsChecked} mods looks fine.</p>
      {:else}
        <div class="findings">
          {#each audit.mods as m (m.uid)}
            <details class="fm" open={audit.mods.length <= 6}>
              <summary>
                <b>{m.name}</b>{#if !m.active}<span class="off">inactive</span>{/if}
                <span class="fc">{m.findings.length} file{m.findings.length === 1 ? "" : "s"}{m.findings.some((f) => !f.fixable) ? ` · ${m.findings.filter((f) => !f.fixable).length} not rebuildable` : ""}</span>
                {#if m.findings.some((f) => f.fixable)}<button class="btn sm" disabled={t?.running} onclick={(e) => { e.preventDefault(); store.fixTextures([[m.uid, []]]); }}>Fix {m.findings.filter((f) => f.fixable).length}</button>{/if}
              </summary>
              <div class="flist">
                {#each m.findings.slice(0, 200) as f}
                  <div class="fr"><span class="mono">{f.rel}</span><span class="why">{problem(f)}{f.hasPng ? " · PNG beside it" : f.fixable ? " · from its own pixels" : " · no source to rebuild from"}</span></div>
                {/each}
                {#if m.findings.length > 200}<div class="hint">…{m.findings.length - 200} more</div>{/if}
              </div>
            </details>
          {/each}
        </div>
      {/if}
    {/if}
  </section>

  <section class="card list">
    <h3>Mods with textures <span class="aside">{shown.length} shown</span>
      <span class="tools">
        <label class="switch sm"><input type="checkbox" bind:checked={onlyActive} />Active only</label>
        <input class="input sm" placeholder="Filter" bind:value={query} />
      </span>
    </h3>
    <div class="hdr"><span>Mod</span><span class="r">PNG</span><span class="r">DDS</span><span class="r">Ours</span><span class="r">Size</span><span></span></div>
    <div class="rows">
      {#each shown.slice(0, 500) as r (r.uid)}
        <div class="row" class:ex={r.excluded}>
          <span class="nm"><b>{r.name}</b>{#if !r.active}<span class="off">inactive</span>{/if}</span>
          <span class="r num">{r.pngs.toLocaleString()}</span>
          <span class="r num">{r.dds.toLocaleString()}</span>
          <span class="r num" class:ok={r.converted > 0}>{r.converted ? r.converted.toLocaleString() : "0"}</span>
          <span class="r num">{r.ddsBytes ? formatBytes(r.ddsBytes) : ""}</span>
          <span class="acts2">
            <button class="ib" title={r.excluded ? "Skipped. Click to allow conversion" : "Skip this mod when converting"} onclick={() => store.setDdsExcluded(r.uid, !r.excluded)}>{@html r.excluded ? I.close : I.check}</button>
            <button class="btn sm" disabled={t?.running || r.excluded || !r.pngs} onclick={() => store.optimizeTextures([r.uid])}>Convert</button>
            <button class="btn sm" disabled={t?.running || !r.converted} onclick={() => revertOne(r)}>Revert</button>
          </span>
        </div>
      {/each}
      {#if shown.length > 500}<div class="hint">…{shown.length - 500} more; narrow the filter.</div>{/if}
      {#if !shown.length}<div class="hint">No mods with PNG textures match.</div>{/if}
    </div>
  </section>
</main>

<style>
  .center { display: flex; flex-direction: column; gap: 12px; min-height: 0; min-width: 0; overflow: hidden auto; padding-bottom: 14px; }
  .tiles { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; }
  .tile .k { font-size: 12px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-3); }
  .tile .v { font-size: 26px; font-weight: 800; letter-spacing: -0.02em; margin-top: 4px; }
  .tile .sub { font-size: 12px; color: var(--text-3); margin-top: 2px; }
  .top { display: grid; grid-template-columns: minmax(0, 3fr) minmax(300px, 2fr); gap: 12px; }
  .lead { margin: 0 0 12px; color: var(--text-2); font-size: 13px; line-height: 1.5; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .cnt { margin-left: 6px; font-size: 11px; opacity: 0.7; }
  .report { margin-top: 12px; font-size: 12.5px; color: var(--text-2); line-height: 1.5; }
  .prog { display: grid; grid-template-columns: 1fr auto; gap: 8px 12px; align-items: center; }
  .bar { grid-column: 1 / -1; height: 8px; border-radius: 4px; background: var(--surface-3); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--amber); border-radius: 4px; transition: width 0.3s; }
  .pl { display: flex; flex-direction: column; gap: 2px; font-size: 12.5px; color: var(--text-2); min-width: 0; }
  .pl b { font-size: 14px; color: var(--text); }
  .cur { color: var(--text-3); font-size: 11.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .errs { margin-top: 10px; font-size: 12.5px; color: var(--text-2); }
  .errs summary { cursor: pointer; font-weight: 600; color: var(--amber); }
  .errlist { margin-top: 6px; max-height: 200px; overflow: hidden auto; display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--text-3); }
  .errlist b { color: var(--text-2); }
  .opt { display: flex; align-items: center; gap: 10px; margin: 8px 0; font-size: 13px; }
  .opt .l { width: 100px; color: var(--text-2); }
  .seg { display: inline-flex; background: var(--surface-2); border-radius: 9px; padding: 3px; gap: 2px; }
  .seg button { padding: 5px 10px; border-radius: 7px; font-size: 12.5px; font-weight: 600; color: var(--text-2); }
  .seg button.on { background: var(--surface-4); color: var(--text); }
  .switch { display: flex; align-items: center; gap: 8px; margin: 8px 0; font-size: 13px; }
  .switch.sm { margin: 0; font-size: 12px; }
  .input.num { width: 70px; height: 28px; }
  .input.sm { height: 28px; font-size: 12.5px; width: 180px; }
  .hint { color: var(--text-3); font-size: 12px; line-height: 1.45; margin: 6px 0 0; }
  .list h3 { display: flex; align-items: center; gap: 10px; }
  .tools { margin-left: auto; display: flex; gap: 12px; align-items: center; font-weight: 500; text-transform: none; letter-spacing: 0; }
  .hdr, .row { display: grid; grid-template-columns: minmax(0, 1fr) 70px 70px 70px 90px 200px; gap: 10px; align-items: center; padding: 0 8px; }
  .hdr { font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-3); height: 28px; }
  .rows { display: flex; flex-direction: column; gap: 1px; }
  .row { height: 38px; border-radius: 9px; font-size: 13px; }
  .row:hover { background: var(--surface-2); }
  .row.ex { opacity: 0.55; }
  .nm { min-width: 0; display: flex; align-items: center; gap: 8px; }
  .nm b { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .off { font-size: 10.5px; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase; color: var(--text-4); background: var(--surface-3); padding: 2px 6px; border-radius: 5px; }
  .r { text-align: right; color: var(--text-2); }
  .r.ok { color: var(--green); font-weight: 700; }
  .acts2 { display: flex; gap: 4px; align-items: center; justify-content: flex-end; opacity: 0.85; }
  .ib { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; color: var(--text-3); }
  .ib:hover { background: var(--surface-3); color: var(--text); }
  .ib :global(svg) { width: 13px; height: 13px; }
  .findings { margin-top: 10px; display: flex; flex-direction: column; gap: 3px; }
  .fm { border-radius: 9px; }
  .fm[open] { background: var(--surface-2); }
  .fm summary { cursor: pointer; display: flex; align-items: center; gap: 10px; padding: 6px 10px; font-size: 13px; color: var(--text-2); }
  .fm summary b { color: var(--text); }
  .fm .fc { color: var(--text-3); font-size: 12px; margin-left: auto; }
  .flist { display: flex; flex-direction: column; gap: 4px; padding: 4px 12px 10px 24px; max-height: 260px; overflow: hidden auto; }
  .fr { display: flex; flex-direction: column; font-size: 12px; }
  .fr .mono { color: var(--text-2); word-break: break-all; }
  .fr .why { color: var(--text-3); font-size: 11.5px; }
  @media (max-width: 1100px) { .top { grid-template-columns: 1fr; } .tiles { grid-template-columns: 1fr 1fr; } }
</style>
