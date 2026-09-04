// Application state for the UI (Svelte 5 runes). One snapshot from the backend, derived
// indexes for fast lookups, and the actions the components call.

import { api, listen } from "./api";
import type { Group, ImportPreview, Issue, ModInfo, Phase, Placement, Rule, Settings, Snapshot, SortResult, Source, UserData, Weight } from "./types";
import { PHASES, primaryUid, severityOf } from "./types";

export type View = "order" | "library" | "downloads" | "analyzer" | "settings";
export type Tab = "active" | "inactive" | "all";
export type ShowOnly = "warning" | "error" | "note" | null;

const ALL_SOURCES: Source[] = ["ludeon", "workshop", "local", "steamcmd", "git"];

class Store {
  snap = $state<Snapshot | null>(null);
  loading = $state(true);
  busy = $state<string | null>(null);
  progress = $state<{ done: number; total: number } | null>(null);
  error = $state<string | null>(null);
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
  showImport = $state(false);

  // ---- derived indexes ----
  mods = $derived(this.snap?.mods ?? []);
  byUid = $derived(new Map(this.mods.map((m) => [m.uid, m])));
  byPackage = $derived(new Map(this.mods.filter((m) => m.packageId).map((m) => [m.packageId, m])));
  active = $derived(this.snap?.active ?? []);
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
    if (q && !(m.name.toLowerCase().includes(q) || m.packageId.includes(q) || m.authors.some((a) => a.toLowerCase().includes(q)) || String(m.publishedFileId ?? "").includes(q))) return false;
    if (this.group && this.snap?.user.modGroups[m.uid] !== this.group) return false;
    if (!this.sources.includes(m.source)) return false;
    if (this.onlyCurrentVersion && m.source !== "ludeon" && !m.supportedVersions.includes(this.snap?.gameVersion.majorMinor ?? "")) return false;
    if (this.showOnly) {
      const list = this.issuesByUid.get(m.uid) ?? [];
      if (!list.some((i) => severityOf(i) === this.showOnly)) return false;
    }
    return true;
  }
  visibleActive = $derived(this.active.map((u) => this.byUid.get(u)!).filter((m) => m && this.matches(m)));
  inactive = $derived(this.mods.filter((m) => !this.activeSet.has(m.uid)).sort((a, b) => a.name.localeCompare(b.name)));
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
    try {
      this.snap = await api.snapshot();
      this.error = null;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
    await listen<{ done: number; total: number }>("scan-progress", (p) => (this.progress = p.done >= p.total ? null : p));
    await listen("state-changed", () => this.refresh());
    await listen<string>("scan-error", (e) => this.say(e, "err"));
  }

  async refresh() {
    this.snap = await api.snapshot();
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
