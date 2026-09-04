// Application state for the UI (Svelte 5 runes). One snapshot from the backend, derived
// indexes for fast lookups, and the actions the components call.

import { api, listen } from "./api";
import type { CollectionPreview, Group, ImportPreview, Issue, ModChange, ModInfo, ModTextures, Phase, Placement, QueueState, RentryPreview, Rule, Settings, Snapshot, SortResult, Source, SteamCmdStatus, TexState, UserData, Weight } from "./types";
import { PHASES, primaryUid, severityOf } from "./types";

export type View = "order" | "library" | "downloads" | "textures" | "analyzer" | "settings";
export type Tab = "active" | "inactive" | "all";
export type ShowOnly = "warning" | "error" | "note" | null;

const ALL_SOURCES: Source[] = ["ludeon", "workshop", "local", "steamcmd", "git"];

class Store {
  snap = $state<Snapshot | null>(null);
  loading = $state(true);
  busy = $state<string | null>(null);
  progress = $state<{ phase: "read" | "inspect"; done: number; total: number } | null>(null);
  error = $state<string | null>(null);
  /** What the loader is doing right now, shown on the startup overlay. */
  step = $state("Connecting to the app");
  toast = $state<{ msg: string; kind: "ok" | "warn" | "err" } | null>(null);

  view = $state<View>("order");
  tab = $state<Tab>("active");
  query = $state("");
  group = $state<string | null>(null);
  sources = $state<Source[]>([...ALL_SOURCES]);
  onlyCurrentVersion = $state(false);
  showOnly = $state<ShowOnly>(null);
  selected = $state<string[]>([]);
  preview = $state<SortResult | null>(null);
  importPreview = $state<ImportPreview | null>(null);
  collectionPreview = $state<CollectionPreview | null>(null);
  rentryPreview = $state<RentryPreview | null>(null);
  showImport = $state(false);
  showChanges = $state(false);
  downloads = $state<QueueState | null>(null);
  /** Why the download manager could not be reached, when it could not. */
  downloadsError = $state<string | null>(null);
  steamcmd = $state<SteamCmdStatus | null>(null);
  /** Texture optimisation job state (from `dds-progress`). */
  tex = $state<TexState | null>(null);
  texOverview = $state<ModTextures[]>([]);
  /** Notices closed for this session (they come back next launch if still true). */
  dismissed = $state<string[]>([]);
  /** uid → the list should scroll to it on the next render. */
  scrollRequest = $state<string | null>(null);

  // ---- derived indexes ----
  mods = $derived.by(() => {
    // Defensive: a repeated uid would break keyed lists, so keep the first of any duplicate.
    const seen = new Set<string>();
    return (this.snap?.mods ?? []).filter((m) => (seen.has(m.uid) ? false : (seen.add(m.uid), true)));
  });
  byUid = $derived(new Map(this.mods.map((m) => [m.uid, m])));
  byPackage = $derived(new Map(this.mods.filter((m) => m.packageId).map((m) => [m.packageId, m])));
  active = $derived.by(() => {
    const seen = new Set<string>();
    return (this.snap?.active ?? []).filter((u) => (seen.has(u) ? false : (seen.add(u), true)));
  });
  activeSet = $derived(new Set(this.active));
  indexOf = $derived(new Map(this.active.map((u, i) => [u, i])));
  placementByUid = $derived(new Map((this.snap?.placements ?? []).map((p) => [p.uid, p])));
  issues = $derived(this.snap?.issues ?? []);
  issuesByUid = $derived.by(() => {
    const map = new Map<string, Issue[]>();
    for (const i of this.issues) {
      const uids = i.kind === "textureCollision" || i.kind === "cycle" || i.kind === "duplicatePackageId" ? i.uids : [i.uid];
      for (const u of uids) {
        if (!map.has(u)) map.set(u, []);
        map.get(u)!.push(i);
      }
    }
    return map;
  });
  groupsById = $derived(new Map((this.snap?.user.groups ?? []).map((g) => [g.id, g])));
  groupOf = (uid: string): Group | undefined => this.groupsById.get(this.snap?.user.modGroups[uid] ?? "");
  weightOf = (m: ModInfo): Weight | undefined => this.snap?.weights[m.packageId];
  pinned = $derived(new Set(this.snap?.user.pinned ?? []));
  showWeight = $derived(this.snap?.settings.showWeight ?? false);
  updateByUid = $derived(new Map((this.snap?.updates ?? []).map((u) => [u.uid, u])));
  changes = $derived(this.snap?.changes ?? []);
  changeByUid = $derived(new Map(this.changes.map((c) => [c.uid, c])));
  listChange = $derived(this.snap?.listChange ?? null);
  changeCounts = $derived.by(() => {
    const n = (k: ModChange["kind"]) => this.changes.filter((c) => c.kind === k).length;
    return { updated: n("updated"), added: n("added"), removed: n("removed"), total: this.changes.length };
  });
  /** "3 updated, 1 new, 2 removed" */
  changeSummary = $derived.by(() => {
    const c = this.changeCounts;
    return [c.updated ? `${c.updated} updated` : "", c.added ? `${c.added} new` : "", c.removed ? `${c.removed} removed` : ""].filter(Boolean).join(", ");
  });
  steamcmdReady = $derived(this.downloads?.steamcmdInstalled ?? false);
  ddsOf = (uid: string) => this.snap?.dds?.[uid];
  ddsTotals = $derived.by(() => {
    let mods = 0, files = 0, ddsBytes = 0, pngBytes = 0, vramBefore = 0;
    for (const s of Object.values(this.snap?.dds ?? {})) { mods++; files += s.count; ddsBytes += s.ddsBytes; pngBytes += s.pngBytes; vramBefore += s.vramBefore; }
    return { mods, files, ddsBytes, pngBytes, vramBefore };
  });
  queueCounts = $derived.by(() => {
    const items = this.downloads?.items ?? [];
    return { queued: items.filter((i) => i.status === "queued" || i.status === "downloading").length, failed: items.filter((i) => i.status === "failed").length, done: items.filter((i) => i.status === "done").length };
  });
  rulesBySubject = $derived.by(() => {
    const map = new Map<string, Rule[]>();
    for (const r of this.snap?.rules ?? []) {
      for (const key of [r.subject, r.target]) {
        if (!key) continue;
        if (!map.has(key)) map.set(key, []);
        map.get(key)!.push(r);
      }
    }
    return map;
  });
  moveOf = $derived.by(() => {
    const map = new Map<string, number>();
    if (this.preview) for (const [uid, from, to] of this.preview.moves) map.set(uid, to - from);
    return map;
  });

  // ---- counts for the stat tiles ----
  stats = $derived.by(() => {
    const errors = this.issues.filter((i) => severityOf(i) === "error").length;
    const warnings = this.issues.filter((i) => severityOf(i) === "warning").length;
    const collisions = this.issues.filter((i) => i.kind === "textureCollision");
    const collidingMods = new Set(collisions.flatMap((c) => c.uids)).size;
    const orderRules = (this.snap?.rules ?? []).filter((r) => (r.kind === "loadAfter" || r.kind === "loadBefore") && this.byPackage.has(r.subject) && r.target && this.byPackage.has(r.target) && this.activeSet.has(this.byPackage.get(r.subject)!.uid) && this.activeSet.has(this.byPackage.get(r.target)!.uid)).length;
    const violations = this.issues.filter((i) => i.kind === "orderViolation").length;
    const satisfied = orderRules ? Math.max(0, orderRules - violations) : 0;
    let share = 0, measured = 0;
    for (const uid of this.active) {
      const m = this.byUid.get(uid);
      const w = m && this.weightOf(m);
      if (w?.share != null) { share += w.share; measured++; }
    }
    return { errors, warnings, collisions: collisions.length, collidingMods, orderRules, violations, satisfied, pct: orderRules ? Math.round((satisfied / orderRules) * 100) : 100, share, measured };
  });

  // ---- filtering ----
  matches(m: ModInfo): boolean {
    const q = this.query.trim().toLowerCase();
    if (q && !((m.name ?? "").toLowerCase().includes(q) || (m.packageId ?? "").includes(q) || (m.authors ?? []).some((a) => a.toLowerCase().includes(q)) || String(m.publishedFileId ?? "").includes(q))) return false;
    if (this.group && this.snap?.user.modGroups[m.uid] !== this.group) return false;
    if (!this.sources.includes(m.source)) return false;
    if (this.onlyCurrentVersion && m.source !== "ludeon" && !(m.supportedVersions ?? []).includes(this.snap?.gameVersion.majorMinor ?? "")) return false;
    if (this.showOnly) {
      const list = this.issuesByUid.get(m.uid) ?? [];
      if (!list.some((i) => severityOf(i) === this.showOnly)) return false;
    }
    return true;
  }
  visibleActive = $derived(this.active.map((u) => this.byUid.get(u)!).filter((m) => m && this.matches(m)));
  inactive = $derived(this.mods.filter((m) => !this.activeSet.has(m.uid)).sort((a, b) => (a.name ?? "").localeCompare(b.name ?? "")));
  visibleInactive = $derived(this.inactive.filter((m) => this.matches(m)));
  sections = $derived.by(() => {
    const out: { phase: (typeof PHASES)[number]; mods: ModInfo[] }[] = [];
    for (const p of PHASES) {
      const mods = this.visibleActive.filter((m) => (this.placementByUid.get(m.uid)?.phase ?? "content") === p.id);
      if (mods.length) out.push({ phase: p, mods });
    }
    return out;
  });
  selectedMod = $derived(this.selected.length ? this.byUid.get(this.selected[this.selected.length - 1]) : undefined);

  // ---- lifecycle ----
  async load() {
    this.loading = true;
    const t0 = performance.now();
    const log = (msg: string) => console.info(`[circinus] ${msg} (+${Math.round(performance.now() - t0)} ms)`);
    // Listeners first: the first snapshot waits for the quick scan, and progress must show meanwhile.
    try {
      await listen<{ phase: "read" | "inspect"; done: number; total: number }>("scan-progress", (p) => (this.progress = p.done >= p.total ? null : p));
      await listen("state-changed", () => this.refresh());
      await listen<string>("scan-error", (e) => this.say(e, "err"));
      await listen<QueueState>("download-progress", (q) => (this.downloads = q));
      await listen<TexState>("dds-progress", (t) => this.onTex(t));
      log("event listeners ready");
    } catch (e) {
      log(`event listeners failed: ${e}`);
      this.error = `Could not connect to the app's event system: ${e}`;
    }
    this.step = "Reading your mods";
    let snap: Snapshot | null = null;
    try {
      snap = await api.snapshot();
      log(`snapshot received: ${snap.mods.length} mods, ${snap.active.length} active, ${snap.issues.length} issues`);
      this.error = null;
    } catch (e) {
      log(`snapshot failed: ${e}`);
      this.error = String(e);
    }
    // Drop the overlay first, then apply the data: even if a panel throws while rendering,
    // its boundary shows the error and the rest of the app stays usable.
    this.loading = false;
    if (snap) {
      this.snap = snap;
      queueMicrotask(() => log("first render scheduled"));
      if (snap.changes.length) setTimeout(() => this.say(`${snap.changes.length} mod${snap.changes.length === 1 ? "" : "s"} changed since you last opened Circinus — ${this.changeSummary}`, "warn"), 400);
      else if (snap.listChange) setTimeout(() => this.say("Your active list was changed outside Circinus", "warn"), 400);
    }
    this.refreshDownloads();
    this.refreshTextures();
  }

  private onTex(t: TexState) {
    const was = this.tex;
    this.tex = t;
    if (was?.running && !t.running && t.report) {
      const r = t.report;
      if (r.reverted || r.bytesFreed) this.say(`Removed ${r.reverted} DDS file${r.reverted === 1 ? "" : "s"} (${(r.bytesFreed / 1e6).toFixed(0)} MB)`);
      else this.say(r.cancelled ? `Stopped after ${r.converted} textures` : `${r.converted} texture${r.converted === 1 ? "" : "s"} converted${r.failed ? `, ${r.failed} failed` : ""}${r.current ? `, ${r.current} already current` : ""} in ${r.seconds}s`, r.failed ? "warn" : "ok");
      this.refreshTextures();
    }
  }
  refreshTextures() {
    api.ddsState().then((t) => (this.tex = t)).catch(() => {});
    api.ddsOverview().then((o) => (this.texOverview = o)).catch((e) => console.warn("[circinus] dds_overview failed", e));
  }
  optimizeTextures(uids: string[]) {
    if (!uids.length) return;
    return this.run("Starting texture job…", async () => {
      await api.ddsStart(uids);
      this.tex = await api.ddsState();
      this.view = "textures";
    });
  }
  revertTextures(uids: string[]) {
    if (!uids.length) return;
    return this.run("Removing DDS files…", async () => {
      await api.ddsRevert(uids);
      await this.refresh();
      this.refreshTextures();
    });
  }
  cancelTextures() {
    api.ddsCancel().catch(() => {});
  }
  setDdsExcluded(uid: string, excluded: boolean) {
    return this.updateUser((u) => {
      const set = new Set(u.ddsExcluded ?? []);
      if (excluded) set.add(uid);
      else set.delete(uid);
      u.ddsExcluded = [...set];
      return u;
    });
  }

  refreshDownloads() {
    api.downloadsState()
      .then((q) => { this.downloads = q; this.downloadsError = null; })
      .catch((e) => { this.downloadsError = String(e); console.warn("[circinus] downloads_state failed", e); });
    api.steamcmdStatus().then((st) => (this.steamcmd = st)).catch(() => {});
  }

  async refresh() {
    const had = this.snap != null;
    const before = new Set(this.changes.map((c) => `${c.kind}:${c.uid}`));
    const beforeList = JSON.stringify(this.listChange);
    this.snap = await api.snapshot();
    api.ddsOverview().then((o) => (this.texOverview = o)).catch(() => {});
    if (!had) return;
    // Something changed while we were open (Steam updated a mod, the game rewrote the list).
    const fresh = this.changes.filter((c) => !before.has(`${c.kind}:${c.uid}`));
    if (fresh.length) {
      const first = fresh[0];
      const what = first.kind === "updated" && first.reasons.includes("workshopUpdate") ? `Steam updated ${first.name}` : first.kind === "added" ? `${first.name} was installed` : first.kind === "removed" ? `${first.name} was removed` : `${first.name} changed on disk`;
      this.say(fresh.length === 1 ? what : `${what} and ${fresh.length - 1} more changed`, "warn");
    } else if (this.listChange && JSON.stringify(this.listChange) !== beforeList) {
      this.say("ModsConfig.xml was changed outside Circinus", "warn");
    }
  }

  say(msg: string, kind: "ok" | "warn" | "err" = "ok") {
    this.toast = { msg, kind };
    setTimeout(() => {
      if (this.toast?.msg === msg) this.toast = null;
    }, kind === "err" ? 8000 : 3500);
  }

  private async run<T>(label: string, f: () => Promise<T>): Promise<T | undefined> {
    this.busy = label;
    try {
      return await f();
    } catch (e) {
      this.say(String(e), "err");
      return undefined;
    } finally {
      this.busy = null;
    }
  }

  private apply(s: Snapshot | undefined) {
    if (s) {
      this.snap = s;
      this.preview = null;
    }
  }

  // ---- selection ----
  select(uid: string, opts: { toggle?: boolean; range?: boolean; list?: string[] } = {}) {
    if (opts.range && this.selected.length && opts.list) {
      const anchor = this.selected[0];
      const a = opts.list.indexOf(anchor), b = opts.list.indexOf(uid);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a < b ? [a, b] : [b, a];
        this.selected = [anchor, ...opts.list.slice(lo, hi + 1).filter((u) => u !== anchor)];
        return;
      }
    }
    if (opts.toggle) {
      this.selected = this.selected.includes(uid) ? this.selected.filter((u) => u !== uid) : [...this.selected, uid];
      return;
    }
    this.selected = [uid];
  }

  // ---- actions ----
  rescan(full = false) {
    return this.run(full ? "Re-reading every mod…" : "Refreshing…", async () => this.apply(await api.rescan(full)));
  }
  setActive(uids: string[]) {
    return this.run("Reordering…", async () => this.apply(await api.setActive(uids)));
  }
  activate(uids: string[], at?: number) {
    return this.run("Activating…", async () => this.apply(await api.activate(uids, at)));
  }
  deactivate(uids: string[]) {
    return this.run("Deactivating…", async () => {
      this.apply(await api.deactivate(uids));
      this.selected = this.selected.filter((u) => !uids.includes(u));
    });
  }
  moveSelected(delta: number) {
    const sel = this.selected.filter((u) => this.activeSet.has(u));
    if (!sel.length) return;
    const order = [...this.active];
    const idxs = sel.map((u) => order.indexOf(u)).sort((a, b) => a - b);
    if (delta < 0 && idxs[0] === 0) return;
    if (delta > 0 && idxs[idxs.length - 1] === order.length - 1) return;
    const seq = delta < 0 ? idxs : [...idxs].reverse();
    for (const i of seq) {
      const j = i + delta;
      [order[i], order[j]] = [order[j], order[i]];
    }
    return this.setActive(order);
  }
  moveTo(uids: string[], index: number) {
    const rest = this.active.filter((u) => !uids.includes(u));
    const before = this.active.slice(0, index).filter((u) => !uids.includes(u)).length;
    const order = [...rest.slice(0, before), ...uids, ...rest.slice(before)];
    return this.setActive(order);
  }
  haloPreview() {
    return this.run("Computing HALO order…", async () => {
      this.preview = await api.halo(false);
      const n = this.preview.moves.length;
      this.say(n ? `HALO would move ${n} mod${n === 1 ? "" : "s"} — review the arrows, then apply` : "Already in HALO order");
    });
  }
  haloApply() {
    return this.run("Applying HALO order…", async () => {
      const r = await api.halo(true);
      this.snap = await api.snapshot();
      this.preview = null;
      this.say(r.moves.length ? `Moved ${r.moves.length} mod${r.moves.length === 1 ? "" : "s"}` : "Already in HALO order");
    });
  }
  save() {
    return this.run("Saving…", async () => {
      const p = await api.save();
      this.snap = await api.snapshot();
      this.say(`Saved ModsConfig.xml`);
      return p;
    });
  }
  async importFrom(path?: string, text?: string) {
    return this.run("Reading list…", async () => {
      this.importPreview = await api.importList(path, text);
    });
  }
  scrollTo(uid: string) {
    this.select(uid);
    this.scrollRequest = uid;
  }
  // ---- downloads ----
  async queueText(text: string) {
    return this.run("Looking up on Steam…", async () => {
      const r = await api.downloadsAddText(text);
      this.downloads = await api.downloadsState();
      this.say(`${r.added} queued${r.skipped.length ? ` · ${r.skipped.length} skipped: ${r.skipped.map(([id, why]) => `${id} (${why})`).join(", ")}` : ""}`, r.skipped.length ? "warn" : "ok");
      return r;
    });
  }
  async queueIds(ids: number[]) {
    if (!ids.length) return;
    return this.run("Looking up on Steam…", async () => {
      const r = await api.downloadsAdd(ids);
      this.downloads = await api.downloadsState();
      this.say(`${r.added} queued${r.skipped.length ? ` · ${r.skipped.length} skipped` : ""}`);
      return r;
    });
  }
  async queueMissing() {
    return this.run("Resolving missing mods…", async () => {
      const [r, unresolved] = await api.downloadsAddMissing();
      this.downloads = await api.downloadsState();
      this.say(`${r.added} queued${unresolved.length ? ` · ${unresolved.length} not in the Steam database: ${unresolved.slice(0, 5).join(", ")}${unresolved.length > 5 ? "…" : ""}` : ""}`, unresolved.length ? "warn" : "ok");
    });
  }
  async removeDownloads(ids: number[]) {
    this.downloads = await api.downloadsRemove(ids);
  }
  async retryFailed() {
    const n = await api.downloadsRetryFailed();
    this.downloads = await api.downloadsState();
    this.say(`${n} retried`);
  }
  async clearFinished() {
    this.downloads = await api.downloadsClearFinished();
  }
  async pauseDownloads(paused: boolean) {
    this.downloads = await api.downloadsPause(paused);
  }
  installSteamCmd() {
    // Show the Downloads view first so the install log streams into view.
    this.view = "downloads";
    return this.run("Installing SteamCMD…", async () => {
      await api.steamcmdInstall();
      this.downloads = await api.downloadsState();
      this.steamcmd = await api.steamcmdStatus();
      this.say("SteamCMD is ready");
    });
  }
  testSteamCmd() {
    return this.run("Testing SteamCMD…", async () => {
      const t = await api.steamcmdTest();
      this.downloads = await api.downloadsState();
      this.say(t.loggedIn ? `SteamCMD works: anonymous login in ${t.seconds}s` : t.stalled ? "SteamCMD produced no output — see the log" : `SteamCMD finished without confirming a login (exit ${t.exitCode ?? "?"})`, t.loggedIn ? "ok" : "err");
      return t;
    });
  }
  acknowledgeChanges() {
    return this.run("Clearing…", async () => {
      this.apply(await api.acknowledgeChanges());
      this.showChanges = false;
    });
  }
  /** Play: through Steam or the executable, per Settings; saves first when asked to. */
  launch() {
    return this.run("Starting RimWorld…", async () => {
      const msg = await api.launchGame();
      if (this.snap?.dirty) await this.refresh();
      this.say(msg);
    });
  }
  dismiss(id: string) {
    if (!this.dismissed.includes(id)) this.dismissed = [...this.dismissed, id];
  }
  checkUpdates() {
    return this.run("Asking the Workshop…", async () => {
      const n = await api.checkUpdates();
      await this.refresh();
      this.say(n ? `${n} mod${n === 1 ? " has" : "s have"} a newer Workshop version` : "Everything is current");
    });
  }
  importCollection(text: string) {
    return this.run("Expanding collection…", async () => {
      this.collectionPreview = await api.importCollection(text);
      this.rentryPreview = null;
      this.importPreview = null;
    });
  }
  importRentry(url: string) {
    return this.run("Fetching Rentry…", async () => {
      const r = await api.importRentry(url);
      this.rentryPreview = r;
      this.importPreview = r.preview;
      this.collectionPreview = null;
    });
  }
  applyCollection(append: boolean) {
    const p = this.collectionPreview;
    if (!p) return;
    return this.run("Applying collection…", async () => {
      const uids = p.installed.map(([, uid]) => uid);
      this.apply(await api.applyImport(uids, append));
      this.say(`${append ? "Appended" : "Activated"} ${uids.length} installed mods${p.missing.length ? `, ${p.missing.length} not installed` : ""}`);
    });
  }
  applyImport(append: boolean) {
    const p = this.importPreview;
    if (!p) return;
    return this.run("Applying list…", async () => {
      this.apply(await api.applyImport(p.uids, append));
      this.importPreview = null;
      this.showImport = false;
      this.say(`${append ? "Appended" : "Imported"} ${p.uids.length} mods${p.missing.length ? `, ${p.missing.length} not installed` : ""}`, p.missing.length ? "warn" : "ok");
    });
  }
  updateUser(patch: (u: UserData) => UserData) {
    if (!this.snap) return;
    const next = patch(structuredClone($state.snapshot(this.snap.user)));
    return this.run("Saving…", async () => this.apply(await api.updateUser(next)));
  }
  setGroup(uids: string[], groupId: string | null) {
    return this.updateUser((u) => {
      for (const uid of uids) {
        if (groupId) u.modGroups[uid] = groupId;
        else delete u.modGroups[uid];
      }
      return u;
    });
  }
  togglePin(uid: string) {
    return this.updateUser((u) => {
      u.pinned = u.pinned.includes(uid) ? u.pinned.filter((x) => x !== uid) : [...u.pinned, uid];
      return u;
    });
  }
  setPhaseOverride(uid: string, phase: Phase | null) {
    return this.updateUser((u) => {
      if (phase) u.phaseOverrides[uid] = phase;
      else delete u.phaseOverrides[uid];
      return u;
    });
  }
  updateSettings(patch: Partial<Settings>) {
    if (!this.snap) return;
    const next: Settings = { ...structuredClone($state.snapshot(this.snap.settings)), ...patch };
    return this.run("Saving settings…", async () => this.apply(await api.updateSettings(next)));
  }
  refreshWeights() {
    return this.run("Fetching Circinus weights…", async () => {
      const n = await api.refreshWeights();
      await this.refresh();
      this.say(`Circinus weight: ${n} mods have figures`);
    });
  }
  updateDatabases() {
    return this.run("Updating databases…", async () => {
      const report = await api.updateDatabases();
      await this.refresh();
      this.say(report.join(" · "));
    });
  }

  phaseInfo(phase: Phase) {
    return PHASES.find((p) => p.id === phase)!;
  }
  placement(uid: string): Placement | undefined {
    return this.placementByUid.get(uid);
  }
  issueUid(i: Issue) {
    return primaryUid(i);
  }
}

export const store = new Store();
