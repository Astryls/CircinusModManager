import { severityOf, type Issue, type ModInfo, type Rule } from "./types";

/** Where a mod's estimated loading time comes from: one clause per part that matters. */
export function explainLoad(m: ModInfo): string[] {
  const c = m.contents;
  const l = c.load;
  if (!l) return [];
  const mb = (b: number) => `${(b / 1e6).toFixed(1)} MB`;
  const out: string[] = [];
  if (c.defs) out.push(`${c.defs} def file${c.defs === 1 ? "" : "s"} (${mb(l.defBytes)})`);
  if (l.patchOps) out.push(`${l.patchOps} patch operation${l.patchOps === 1 ? "" : "s"}${l.heavyOps ? `, ${l.heavyOps} scanning the whole document` : ""}`);
  if (l.pngPixels) out.push(`${(l.pngPixels / 1e6).toFixed(1)} megapixels of PNG to decode (${mb(l.pngBytes)})`);
  if (l.ddsBytes) out.push(`${mb(l.ddsBytes)} of DDS`);
  if (c.assemblies) out.push(`${c.assemblies} assembl${c.assemblies === 1 ? "y" : "ies"} (${mb(l.dllBytes)})`);
  if (l.soundBytes) out.push(`${mb(l.soundBytes)} of sound`);
  return out;
}

/** Human sentence for an issue, from the point of view of the mod it is attached to. */
export function describe(i: Issue, byUid: Map<string, ModInfo>, viewer?: string): string {
  const n = (uid: string) => byUid.get(uid)?.name ?? uid;
  switch (i.kind) {
    case "missingDependency": {
      const dep = i.displayName ?? i.dependency;
      if (!i.installedUid) return `Needs ${dep}, which is not installed.`;
      const other = byUid.get(i.installedUid);
      if (other?.invalid) return `Needs ${dep}. It is in your mod folder, but Circinus cannot read it: ${other.invalid}`;
      return `Needs ${dep}, which is installed but not active.`;
    }
    case "incompatible": {
      const other = viewer === i.otherUid ? i.uid : i.otherUid;
      return `Does not work together with ${n(other)}. Both are active.`;
    }
    case "orderViolation": {
      if (i.comment === "Needs it") {
        return viewer && viewer === i.targetUid ? `${n(i.uid)} needs this mod, so it should load after it.` : `Needs ${n(i.targetUid)}, so it should load after it.`;
      }
      const who = viewer && viewer === i.targetUid ? n(i.uid) : n(i.targetUid);
      const verb = viewer && viewer === i.targetUid ? (i.rule === "loadAfter" ? "should load before" : "should load after") : i.rule === "loadAfter" ? "should load after" : "should load before";
      const src = i.source === "community" ? "Community rule" : i.source === "user" ? "Your rule" : i.source === "manifest" ? "Manifest.xml" : "About.xml";
      return `${src}: ${viewer && viewer === i.targetUid ? who : "this mod"} ${verb} ${viewer && viewer === i.targetUid ? "this mod" : who}.${i.comment ? ` (${i.comment})` : ""}`;
    }
    case "versionMismatch":
      return i.supported.length ? `Made for ${i.supported.join(", ")}, not for this game version.` : "Does not say which game versions it supports.";
    case "cycle": {
      // Every step, and where it came from. A loop is only actionable if you can see which rule
      // to change, and the rule to change is almost never in the mod you are looking at: it is
      // usually one line in a community database or one line in somebody's About.xml.
      const where = (r: Rule) => (r.source === "community" ? "community database" : r.source === "user" ? "your rule" : r.source === "manifest" ? "Manifest.xml" : r.source === "halo" ? "worked out by HALO" : "the mod's About.xml");
      const steps = i.rules.map((r, k) => `  ${n(i.uids[k])} before ${n(i.uids[(k + 1) % i.uids.length])} — ${where(r)}${r.comment ? `, ${r.comment}` : ""}`);
      const cut = i.cut
        ? `Sorting cannot obey all of them, so one was set aside: ${n(i.cut.subject === (i.uids[0] ?? "") ? i.uids[0] : i.cut.subject)} ${i.cut.kind === "loadBefore" ? "before" : "after"} ${i.cut.target ?? "?"}, from the ${where(i.cut)}. Everything else on the loop was obeyed.`
        : "Sorting could not break the loop.";
      return `Rules form a loop:\n${steps.join("\n")}\n\n${cut}`;
    }
    case "textureCollision": {
      const winner = n(i.winnerUid);
      const others = i.uids.filter((u) => u !== i.winnerUid).map(n).join(", ");
      return viewer === i.winnerUid ? `Its ${i.path} wins over ${others}.` : `Its ${i.path} is replaced again by ${winner}, which loads later.`;
    }
    case "misplacedOptimization":
      return `Works best at the end of the list, but ${i.afterUids.length} other mod${i.afterUids.length === 1 ? "" : "s"} load after it (${i.afterUids.slice(0, 3).map(n).join(", ")}${i.afterUids.length > 3 ? " and more" : ""}). Sort with HALO to fix this.`;
    case "duplicatePackageId":
      return `${i.packageId} is installed ${i.uids.length} times. RimWorld picks one copy and ignores the rest.`;
    case "missingPackageId":
      return "About.xml has no packageId, so no rule can refer to it.";
    case "invalid":
      return i.reason;
    case "aboveOfficial":
      return `Sits above ${n(i.officialUid)}. A def can only inherit from mods loaded before it, so this mod would lose its parents and the game would fail to load. Move it below the game and all DLC.`;
    case "ruleIgnored": {
      const src = i.source === "community" ? "community rule" : i.source === "user" ? "your rule" : i.source === "manifest" ? "Manifest.xml rule" : "About.xml rule";
      return `HALO set aside the ${src} "${i.rule === "loadBefore" ? "load before" : "load after"} ${n(i.targetUid)}" because ${i.reason}.`;
    }
    default:
      return `Needs attention (${(i as { kind: string }).kind}).`;
  }
}

/** One line for the attention banner: the most pressing issue in the list. */
export function headline(issues: Issue[], byUid: Map<string, ModInfo>): { title: string; detail: string; kind: "error" | "warning" | "note" } | null {
  const n = (uid: string) => byUid.get(uid)?.name ?? uid;
  const above = issues.filter((i) => i.kind === "aboveOfficial");
  if (above.length) {
    const first = above[0] as Extract<Issue, { kind: "aboveOfficial" }>;
    const names = above.map((i) => n(i.kind === "aboveOfficial" ? i.uid : "")).slice(0, 3).join(", ");
    return { title: `${names}${above.length > 3 ? ` and ${above.length - 3} more` : ""} ${above.length === 1 ? "sits" : "sit"} above ${n(first.officialUid)}`, detail: "Their defs cannot find their parents there, and the game will reset the list when it starts. Sort with HALO, or move them below the game and all DLC.", kind: "error" };
  }
  const cycle = issues.find((i) => i.kind === "cycle");
  if (cycle && cycle.kind === "cycle") return { title: "Some rules contradict each other", detail: cycle.chain, kind: "error" };
  const inc = issues.find((i) => i.kind === "incompatible");
  if (inc && inc.kind === "incompatible") return { title: `${n(inc.uid)} and ${n(inc.otherUid)} do not work together`, detail: "Both are active. Deactivate one of them before playing.", kind: "error" };
  const dep = issues.find((i) => i.kind === "missingDependency");
  if (dep && dep.kind === "missingDependency") return { title: `${n(dep.uid)} needs ${dep.displayName ?? dep.dependency}`, detail: dep.installedUid ? "It is installed but not active. Activate it." : "It is not installed.", kind: "error" };
  const opt = issues.find((i) => i.kind === "misplacedOptimization");
  if (opt && opt.kind === "misplacedOptimization") return { title: `${n(opt.uid)} works best at the end of the list`, detail: `${opt.afterUids.length} mod${opt.afterUids.length === 1 ? "" : "s"} load after it. Sort with HALO to fix this, or close this note.`, kind: "warning" };
  const coll = issues.filter((i) => i.kind === "textureCollision");
  if (coll.length) {
    const first = coll[0] as Extract<Issue, { kind: "textureCollision" }>;
    const winners = new Map<string, number>();
    for (const c of coll) if (c.kind === "textureCollision") winners.set(c.winnerUid, (winners.get(c.winnerUid) ?? 0) + 1);
    const [top, count] = [...winners.entries()].sort((a, b) => b[1] - a[1])[0];
    const losers = [...new Set(coll.flatMap((c) => (c.kind === "textureCollision" ? c.uids.filter((u) => u !== c.winnerUid) : [])))].map(n);
    return { title: `${n(top)} replaces ${count} texture${count === 1 ? "" : "s"} that ${losers.slice(0, 2).join(" and ")} already replaced`, detail: `${n(top)} loads later, so its files win. Pin it if that is what you want.${first ? "" : ""}`, kind: "note" };
  }
  const ov = issues.find((i) => i.kind === "orderViolation");
  if (ov && ov.kind === "orderViolation") return { title: `${n(ov.uid)} is out of order`, detail: describe(ov, byUid), kind: "warning" };
  return null;
}

/** The same line, about one mod: what Review has just moved to.
 *
 * `headline` reads the whole list and names whichever issue is worst in it, which is the right
 * answer for a banner nobody has interacted with and the wrong one the moment Review starts
 * walking: pressing Next moved the list to the next mod while the words above it went on
 * describing something else entirely. */
export function headlineFor(uid: string, mine: Issue[], byUid: Map<string, ModInfo>): { title: string; detail: string; kind: "error" | "warning" | "note" } | null {
  if (!mine.length) return null;
  const rank = { error: 0, warning: 1, note: 2 };
  const worst = [...mine].sort((a, b) => rank[severityOf(a)] - rank[severityOf(b)])[0];
  // Scoped to the one issue, `headline` writes the same sentence about this mod that it would
  // have written about the list's worst, so the two banners read alike.
  const name = byUid.get(uid)?.name ?? uid;
  const h = headline([worst], byUid);
  // Keep the familiar wording where it is already about this mod. Several issues have two sides
  // and `headline` always writes them from the other one's: the mod whose textures are being
  // overwritten would have been handed a sentence praising the mod overwriting them, and Review
  // would have looked like it had gone to the wrong row.
  if (h && h.title.startsWith(name)) return h;
  return { title: name, detail: describe(worst, byUid, uid).replace(/\s*\n\s*/g, " "), kind: severityOf(worst) };
}
