<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I, sevIcon } from "$lib/icons";
  import { describe } from "$lib/describe";
  import { api, assetUrl, openUrl, revealPath } from "$lib/api";
  import { BAND_LABEL, PHASES, SOURCE_LABEL, describeChange, formatBytes, initials, severityOf, type Phase, type Rule } from "$lib/types";

  const m = $derived(store.selectedMod);
  const placement = $derived(m ? store.placement(m.uid) : undefined);
  const issues = $derived(m ? store.issuesByUid.get(m.uid) ?? [] : []);
  const rules = $derived(m ? (store.rulesBySubject.get(m.packageId) ?? []).filter((r) => r.kind !== "incompatible" || true) : []);
  const delta = $derived(m ? store.moveOf.get(m.uid) : undefined);
  const weight = $derived(m ? store.weightOf(m) : undefined);
  const group = $derived(m ? store.groupOf(m.uid) : undefined);
  const isActive = $derived(m ? store.activeSet.has(m.uid) : false);
  const change = $derived(m ? store.changeByUid.get(m.uid) : undefined);
  const update = $derived(m ? store.updateByUid.get(m.uid) : undefined);
  const canRedownload = $derived(!!m?.publishedFileId && m.source !== "ludeon");
  const dds = $derived(m ? store.ddsOf(m.uid) : undefined);
  const ddsExcluded = $derived(m ? (store.snap?.user.ddsExcluded ?? []).includes(m.uid) : false);
  const GRAD: Record<Phase, [string, string]> = { core: ["#3b5fd9", "#1b2a5c"], prepatch: ["#8b6cf0", "#3a2a6e"], framework: ["#2ea59e", "#12403e"], content: ["#3fb865", "#173f24"], patch: ["#d9508f", "#5a1f3c"], texture: ["#e39b3a", "#5d3a0f"], optimization: ["#f07a4d", "#5d2a17"] };
  let preview = $state<string>("");
  let description = $state<string>("");
  $effect(() => {
    const p = m?.preview;
    preview = "";
    if (p) assetUrl(p).then((u) => (preview = u));
  });
  $effect(() => {
    const uid = m?.uid;
    description = "";
    if (uid) api.description(uid).then((d) => { if (m?.uid === uid) description = d; }).catch(() => {});
  });

  function ruleLine(r: Rule): { type: string; name: string; ok: boolean } {
    const mine = r.subject === m?.packageId;
    const otherId = mine ? r.target : r.subject;
    const other = otherId ? store.byPackage.get(otherId) : undefined;
    const name = other?.name ?? otherId ?? "";
    if (r.kind === "loadTop") return { type: "top", name: "Load at the top", ok: true };
    if (r.kind === "loadBottom") return { type: "bottom", name: "Load at the bottom", ok: true };
    if (r.kind === "incompatible") return { type: "never", name, ok: !(other && store.activeSet.has(other.uid)) };
    const after = (r.kind === "loadAfter") === mine; // from this mod's point of view
    const mi = m ? store.indexOf.get(m.uid) : undefined;
    const oi = other ? store.indexOf.get(other.uid) : undefined;
    const ok = mi == null || oi == null ? true : after ? mi > oi : mi < oi;
    return { type: after ? "after" : "before", name, ok };
  }
  const srcLabel: Record<string, string> = { about: "About", manifest: "Manifest", community: "Community", user: "Mine", halo: "HALO" };
  function workshopUrl() {
    return m?.publishedFileId ? `https://steamcommunity.com/sharedfiles/filedetails/?id=${m.publishedFileId}` : m?.url;
  }
</script>

<aside class="inspector">
  {#if m}
    <section class="card">
      <div class="hero">
        {#if preview}
          <img class="thumb" src={preview} alt="" />
        {:else}
          {@const [c1, c2] = GRAD[placement?.phase ?? "content"]}
          <span class="thumb mono-tile" style="--c1:{c1};--c2:{c2}">{initials(m.name)}</span>
        {/if}
        <div class="t"><b>{m.name}</b><span>{m.authors.join(", ") || "Unknown author"}</span><span class="mono">{m.packageId || "no packageId"}</span></div>
      </div>
      <dl class="kv">
        <dt>Source</dt><dd>{SOURCE_LABEL[m.source]}{#if m.publishedFileId} · <button class="lnk mono" onclick={() => openUrl(workshopUrl()!)}>{m.publishedFileId}</button>{/if}</dd>
        <dt>Versions</dt><dd>{m.supportedVersions.join(" · ") || "—"}{m.modVersion ? ` · v${m.modVersion}` : ""}</dd>
        <dt>Size</dt><dd class="num">{formatBytes(m.contents.sizeBytes)}</dd>
        <dt>Contains</dt><dd>{[m.contents.assemblies ? `${m.contents.assemblies} assembl${m.contents.assemblies === 1 ? "y" : "ies"}` : null, m.contents.defs ? `${m.contents.defs} def files` : null, m.contents.patches ? `${m.contents.patches} patches` : null, m.contents.textures + m.contents.dds ? `${(m.contents.textures + m.contents.dds).toLocaleString()} textures` : null].filter(Boolean).join(", ") || "nothing loadable"}</dd>
        <dt>On disk</dt><dd>{m.modified ? new Date(m.modified * 1000).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" }) : "—"}</dd>
        {#if m.contents.textures + m.contents.dds > 0 || dds}
          <dt>Textures</dt>
          <dd>{m.contents.textures.toLocaleString()} PNG{m.contents.dds ? ` · ${m.contents.dds.toLocaleString()} DDS` : ""}{dds ? ` (${dds.count.toLocaleString()} by Circinus, ${formatBytes(dds.ddsBytes)})` : ""}{ddsExcluded ? " · excluded" : ""}</dd>
        {/if}
        <dt>Group</dt>
        <dd>
          <select class="sel" value={group?.id ?? ""} onchange={(e) => store.setGroup(store.selected.length > 1 ? store.selected : [m.uid], (e.currentTarget as HTMLSelectElement).value || null)}>
            <option value="">None</option>
            {#each store.snap?.user.groups ?? [] as g}<option value={g.id}>{g.name}</option>{/each}
          </select>
        </dd>
      </dl>
    </section>

    {#if change || update}
      <section class="card changed">
        <h3>{change ? "Changed since last launch" : "Newer on the Workshop"}</h3>
        {#if change}<div class="chg"><span class="flag chg">{@html change.kind === "added" ? I.plus : I.change}</span><span>{describeChange(change)}</span></div>{/if}
        {#if update}<div class="chg"><span class="flag note">{@html I.up}</span><span>Workshop version from {new Date(update.remoteUpdated * 1000).toLocaleDateString()}; yours is from {new Date(update.localModified * 1000).toLocaleDateString()}{update.source === "workshop" ? " — Steam updates it when the game next starts" : ""}</span></div>{/if}
        <div class="acts two">
          {#if m.publishedFileId}<button class="btn" onclick={() => openUrl(`https://steamcommunity.com/sharedfiles/filedetails/changelog/${m.publishedFileId}`)}>Changelog</button>{/if}
          {#if canRedownload}<button class="btn" onclick={() => store.queueIds([m.publishedFileId!])}>{@html I.download}Re-download</button>{/if}
        </div>
      </section>
    {/if}

    <section class="card halo">
      <h3>HALO placement</h3>
      {#if placement}
        <div class="ph2"><span class="dot c-{store.phaseInfo(placement.phase).color}"></span>{store.phaseInfo(placement.phase).name}<span class="why">{placement.reason}</span></div>
      {:else}
        <div class="ph2">Not active</div>
      {/if}
      <div class="move">
        {#if delta}
          HALO would move it <b>{Math.abs(delta)} row{Math.abs(delta) === 1 ? "" : "s"} {delta > 0 ? "down" : "up"}</b>.
        {:else if store.preview}
          Already where HALO would put it.
        {:else}
          Preview with <b>Sort with HALO</b> to see where it would go.
        {/if}
      </div>
      <div class="ctl">
        <label class="switch"><input type="checkbox" checked={store.pinned.has(m.uid)} onchange={() => store.togglePin(m.uid)} />Pin position</label>
        <select class="sel" value={store.snap?.user.phaseOverrides[m.uid] ?? ""} onchange={(e) => store.setPhaseOverride(m.uid, ((e.currentTarget as HTMLSelectElement).value || null) as Phase | null)} title="Force a phase for this mod">
          <option value="">Phase: automatic</option>
          {#each PHASES as p}<option value={p.id}>Phase: {p.name}</option>{/each}
        </select>
      </div>
    </section>

    {#if store.showWeight}
      <section class="card">
        <h3>Circinus weight <span class="aside">{weight?.origin === "local" ? "your runs" : weight ? "circinus.sh" : ""}</span></h3>
        {#if weight}
          <div class="wrow">
            <span class="band {weight.band}">{BAND_LABEL[weight.band]}</span>
            <b class="num">{weight.share != null ? `${weight.share.toFixed(2)}%` : "—"}</b>
            <span class="wcap">of measured frame time{weight.ranked ? "" : " · not ranked yet"}</span>
          </div>
          <div class="wmeta">
            {#if weight.measured != null}{weight.measured} measured runs{/if}{#if weight.installs != null} · {weight.installs} installs{/if}{#if weight.withheld} · figures withheld by author{/if}
            {#if weight.netLow != null && weight.netHigh != null}<br />Net {weight.netLow.toFixed(2)}–{weight.netHigh.toFixed(2)}% (skip-capable patches){/if}
          </div>
          <button class="lnk" onclick={() => openUrl(`https://circinus.sh/mods/${encodeURIComponent(m.packageId)}`)}>Open on circinus.sh</button>
        {:else}
          <div class="wmeta">No figures for this mod yet. Circinus ranks a mod after 25 clean runs from 10 installs.</div>
        {/if}
      </section>
    {/if}

    <section class="card">
      <h3>Rules <span class="aside">{rules.filter((r) => ruleLine(r).ok).length} of {rules.length} met</span></h3>
      {#if rules.length}
        <div class="rules">
          {#each rules as r}
            {@const l = ruleLine(r)}
            <div class="rule" title={r.comment ?? ""}>
              <span class="st" class:ok={l.ok} class:bad={!l.ok}>{@html l.ok ? I.check : I.error}</span>
              <span class="ty">{l.type}</span>
              <span class="nm">{l.name}</span>
              <span class="srcb {r.source}">{srcLabel[r.source]}</span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="muted">No ordering rules mention this mod.</div>
      {/if}
    </section>

    {#if issues.length}
      <section class="card issues">
        <h3>Needs attention</h3>
        {#each issues.slice(0, 12) as i}
          <div class="it"><span class="flag {severityOf(i)}">{@html sevIcon[severityOf(i)]}</span><span>{describe(i, store.byUid, m.uid)}</span></div>
        {/each}
        {#if issues.length > 12}<div class="muted">and {issues.length - 12} more in the Analyzer</div>{/if}
      </section>
    {/if}

    <section class="card">
      <h3>Actions</h3>
      <div class="acts">
        <button class="btn" onclick={() => revealPath(m.path)}>Open folder</button>
        <button class="btn" disabled={!workshopUrl()} onclick={() => openUrl(workshopUrl()!)}>Workshop page</button>
        {#if canRedownload && !change && !update}<button class="btn" title="Fetch a fresh copy from the Workshop with SteamCMD (into your Mods folder)" onclick={() => store.queueIds([m.publishedFileId!])}>{@html I.download}Re-download</button>{/if}
        {#if m.contents.textures > 0 && !ddsExcluded && m.source !== "ludeon"}<button class="btn" title="Convert this mod's PNG textures to DDS" disabled={store.tex?.running} onclick={() => store.optimizeTextures([m.uid])}>{@html I.image}Optimise textures</button>{/if}
        {#if dds}<button class="btn" title="Delete the DDS files Circinus made for this mod" disabled={store.tex?.running} onclick={() => store.revertTextures([m.uid])}>Remove DDS</button>{/if}
        {#if isActive}
          <button class="btn" onclick={() => store.moveSelected(-1)}>Move up</button>
          <button class="btn" onclick={() => store.moveSelected(1)}>Move down</button>
          <button class="btn danger" onclick={() => store.deactivate(store.selected.length > 1 ? store.selected : [m.uid])}>Deactivate</button>
        {:else}
          <button class="btn primary" onclick={() => store.activate(store.selected.length > 1 ? store.selected : [m.uid])}>Activate</button>
        {/if}
      </div>
      {#if description}<details class="desc"><summary>Description</summary><p>{description.replace(/<[^>]+>/g, "")}</p></details>{/if}
    </section>
  {:else}
    <section class="card empty-card">
      <div class="label">Inspector</div>
      <p>Select a mod to see its rules, HALO placement and what needs attention. Shift-click selects a range, Ctrl-click adds to the selection. Alt ↑/↓ moves the selection, Delete deactivates it.</p>
    </section>
  {/if}
</aside>

<style>
  .inspector { display: flex; flex-direction: column; gap: 12px; min-height: 0; overflow: auto; padding-bottom: 4px; }
  .hero { display: flex; gap: 12px; align-items: flex-start; }
  .thumb { width: 64px; height: 64px; border-radius: 12px; flex: none; object-fit: cover; box-shadow: var(--shadow-card); }
  .mono-tile { display: grid; place-items: center; font: 800 18px var(--font); color: #fff; letter-spacing: -0.02em; background: linear-gradient(150deg, var(--c1), var(--c2)); }
  .hero .t { min-width: 0; }
  .hero .t b { display: block; font-size: 16px; font-weight: 800; letter-spacing: -0.01em; line-height: 1.15; }
  .hero .t span { display: block; color: var(--text-2); font-size: 12.5px; margin-top: 2px; }
  .hero .t .mono { margin-top: 6px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .kv { display: grid; grid-template-columns: auto 1fr; gap: 6px 12px; font-size: 12.5px; margin: 12px 0 0; }
  .kv dt { color: var(--text-3); margin: 0; } .kv dd { margin: 0; text-align: right; color: var(--text-2); min-width: 0; }
  .lnk { color: var(--blue); font-weight: 600; }
  .lnk:hover { text-decoration: underline; }
  .sel { height: 26px; border: 0; border-radius: 7px; background: var(--surface-3); color: var(--text); font-size: 12px; padding: 0 8px; max-width: 100%; }
  .halo .ph2 { display: flex; align-items: center; gap: 8px; font-weight: 600; flex-wrap: wrap; }
  .halo .why { color: var(--text-3); font-weight: 500; font-size: 12px; margin-left: auto; text-align: right; }
  .halo .move { margin-top: 10px; background: var(--surface-2); border-radius: 10px; padding: 10px 12px; font-size: 12.5px; color: var(--text-2); }
  .halo .move b { color: var(--text); }
  .ctl { display: flex; justify-content: space-between; align-items: center; gap: 8px; margin-top: 10px; font-size: 12.5px; }
  .ctl .switch { font-size: 12.5px; white-space: nowrap; }
  .wrow { display: flex; align-items: baseline; gap: 10px; }
  .wrow b { font-size: 22px; font-weight: 800; letter-spacing: -0.02em; }
  .wcap { font-size: 12px; color: var(--text-3); }
  .wmeta { font-size: 12px; color: var(--text-3); margin: 8px 0; line-height: 1.5; }
  .rules { display: flex; flex-direction: column; gap: 2px; }
  .rule { display: grid; grid-template-columns: 16px 44px minmax(0, 1fr) auto; gap: 7px; align-items: center; height: 30px; padding: 0 6px; border-radius: 8px; font-size: 12.5px; }
  .rule:hover { background: var(--surface-2); }
  .st { width: 16px; height: 16px; border-radius: 50%; display: grid; place-items: center; }
  .st :global(svg) { width: 9px; height: 9px; }
  .st.ok { background: var(--green-soft); color: var(--green); } .st.bad { background: var(--red-soft); color: var(--red); }
  .ty { font: 600 10.5px var(--mono); color: var(--text-3); text-transform: uppercase; }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .srcb { font-size: 10.5px; font-weight: 700; letter-spacing: 0.04em; padding: 2px 6px; border-radius: 5px; background: var(--surface-3); color: var(--text-3); }
  .srcb.community { color: var(--teal); } .srcb.user { color: var(--violet); } .srcb.halo { color: var(--amber); }
  .issues .it { display: flex; gap: 10px; padding: 8px 0; font-size: 12.5px; color: var(--text-2); line-height: 1.4; }
  .issues .it + .it { border-top: 1px solid rgba(255, 255, 255, 0.05); }
  .issues .flag { margin-top: 1px; }
  .changed .chg { display: flex; gap: 10px; align-items: flex-start; font-size: 12.5px; color: var(--text-2); line-height: 1.45; padding: 4px 0; }
  .changed .chg .flag { margin-top: 2px; }
  .changed :global(.flag.chg) { background: var(--amber-soft); color: var(--amber); }
  .changed .acts { margin-top: 8px; }
  .acts { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .acts .btn { height: 32px; font-size: 12.5px; background: var(--surface-2); }
  .acts .btn.primary { background: var(--amber); }
  .desc { margin-top: 12px; font-size: 12.5px; color: var(--text-2); }
  .desc summary { cursor: pointer; font-weight: 600; color: var(--text-3); }
  .desc p { white-space: pre-wrap; line-height: 1.5; max-height: 260px; overflow: auto; margin: 8px 0 0; user-select: text; }
  .muted { color: var(--text-3); font-size: 12.5px; }
  .empty-card p { color: var(--text-3); font-size: 13px; line-height: 1.5; margin: 8px 0 0; }
</style>
