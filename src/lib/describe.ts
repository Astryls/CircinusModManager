import type { Issue, ModInfo } from "./types";

/** Human sentence for an issue, from the point of view of the mod it is attached to. */
export function describe(i: Issue, byUid: Map<string, ModInfo>, viewer?: string): string {
  const n = (uid: string) => byUid.get(uid)?.name ?? uid;
  switch (i.kind) {
    case "missingDependency":
      return i.installedUid
        ? `Needs ${i.displayName ?? i.dependency}, which is installed but not active.`
        : `Needs ${i.displayName ?? i.dependency}, which is not installed.`;
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
    case "cycle":
      return `Rules form a loop: ${i.chain}. The weakest rule was set aside so sorting could finish.`;
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
