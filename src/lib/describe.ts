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
      return `Incompatible with ${n(other)} — both are active.`;
    }
    case "orderViolation": {
      const who = viewer && viewer === i.targetUid ? n(i.uid) : n(i.targetUid);
      const verb = viewer && viewer === i.targetUid ? (i.rule === "loadAfter" ? "should load before" : "should load after") : i.rule === "loadAfter" ? "should load after" : "should load before";
      const src = i.source === "community" ? "Community rule" : i.source === "user" ? "Your rule" : i.source === "manifest" ? "Manifest.xml" : "About.xml";
      return `${src}: ${viewer && viewer === i.targetUid ? who : "this mod"} ${verb} ${viewer && viewer === i.targetUid ? "this mod" : who}.${i.comment ? ` (${i.comment})` : ""}`;
    }
    case "versionMismatch":
      return i.supported.length ? `Lists ${i.supported.join(", ")} — not ${"the current game version"}.` : "Does not list any supported version.";
    case "cycle":
      return `Rules form a loop: ${i.chain}. The weakest rule was set aside to keep sorting.`;
    case "textureCollision": {
      const winner = n(i.winnerUid);
      const others = i.uids.filter((u) => u !== i.winnerUid).map(n).join(", ");
      return viewer === i.winnerUid ? `Wins ${i.path} over ${others}.` : `${i.path} is replaced again by ${winner}, which loads later.`;
    }
    case "misplacedOptimization":
      return `Should be the last kind of mod; ${i.afterUids.length} other${i.afterUids.length === 1 ? "" : "s"} load after it (${i.afterUids.slice(0, 3).map(n).join(", ")}${i.afterUids.length > 3 ? "…" : ""}).`;
    case "duplicatePackageId":
      return `${i.packageId} is installed ${i.uids.length} times; the copy RimWorld picks is ambiguous.`;
    case "missingPackageId":
      return "About.xml has no packageId, so rules cannot refer to it.";
    case "invalid":
      return i.reason;
  }
}

/** One line for the attention banner: the most pressing issue in the list. */
export function headline(issues: Issue[], byUid: Map<string, ModInfo>): { title: string; detail: string; kind: "error" | "warning" | "note" } | null {
  const n = (uid: string) => byUid.get(uid)?.name ?? uid;
  const cycle = issues.find((i) => i.kind === "cycle");
  if (cycle && cycle.kind === "cycle") return { title: "Rules contradict each other", detail: cycle.chain, kind: "error" };
  const inc = issues.find((i) => i.kind === "incompatible");
  if (inc && inc.kind === "incompatible") return { title: `${n(inc.uid)} and ${n(inc.otherUid)} are incompatible`, detail: "Both are active. Deactivate one of them before playing.", kind: "error" };
  const dep = issues.find((i) => i.kind === "missingDependency");
  if (dep && dep.kind === "missingDependency") return { title: `${n(dep.uid)} needs ${dep.displayName ?? dep.dependency}`, detail: dep.installedUid ? "It is installed but inactive — activate it." : "It is not installed.", kind: "error" };
  const opt = issues.find((i) => i.kind === "misplacedOptimization");
  if (opt && opt.kind === "misplacedOptimization") return { title: `${n(opt.uid)} should load last`, detail: `${opt.afterUids.length} mod${opt.afterUids.length === 1 ? "" : "s"} load after it. Sort with HALO fixes this.`, kind: "warning" };
  const coll = issues.filter((i) => i.kind === "textureCollision");
  if (coll.length) {
    const first = coll[0] as Extract<Issue, { kind: "textureCollision" }>;
    const winners = new Map<string, number>();
    for (const c of coll) if (c.kind === "textureCollision") winners.set(c.winnerUid, (winners.get(c.winnerUid) ?? 0) + 1);
    const [top, count] = [...winners.entries()].sort((a, b) => b[1] - a[1])[0];
    const losers = [...new Set(coll.flatMap((c) => (c.kind === "textureCollision" ? c.uids.filter((u) => u !== c.winnerUid) : [])))].map(n);
    return { title: `${n(top)} replaces ${count} texture${count === 1 ? "" : "s"} that ${losers.slice(0, 2).join(" and ")} already replaced`, detail: `${n(top)} loads later, so it wins. Pin it if that's intended.${first ? "" : ""}`, kind: "note" };
  }
  const ov = issues.find((i) => i.kind === "orderViolation");
  if (ov && ov.kind === "orderViolation") return { title: `${n(ov.uid)} is out of order`, detail: describe(ov, byUid), kind: "warning" };
  return null;
}
