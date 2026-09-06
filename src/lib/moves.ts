/**
 * What actually changes between the order you have and the one HALO proposes.
 *
 * A sorted list reports far more "moves" than it makes decisions. Lift one mod from #900 to #12
 * and 888 other mods have a new number without HALO having decided anything about them: they were
 * passed. On a real list that is the difference between 317 rows of "-1" and the twenty-odd
 * relocations a person can actually read.
 *
 * So the two orders are compared the way a diff compares two files. The longest run of mods that
 * keep their order relative to one another is the backbone — those mods stay. Everything else is
 * a relocation: a mod HALO lifted out and put somewhere new. The backbone is what a relocation is
 * described against, since a mod moving "after Harmony" is only meaningful if Harmony itself is
 * not moving.
 */
import type { Phase } from "./types";

export interface Relocation {
  uid: string;
  /** Its place in the order you have now, and in the one HALO proposes. Both zero-based. */
  from: number;
  to: number;
  fromPhase: Phase;
  toPhase: Phase;
  /** The mods that stay put on either side of where it lands, and the one it used to follow. */
  afterUid?: string;
  beforeUid?: string;
  wasAfterUid?: string;
  /** HALO's own words for why it files the mod where it does. */
  reason: string;
}

export interface MoveReport {
  /** Every mod HALO lifts out of its place, in the order they end up in. */
  relocations: Relocation[];
  /** Mods with a new number only because a relocation passed them. */
  drift: number;
  /** How many relocations change the phase a mod is filed under. */
  crossPhase: number;
  /** The phases a relocation leaves and lands in, so the board can draw only what is involved. */
  fromPhases: Phase[];
  toPhases: Phase[];
}

/**
 * Indices of a longest increasing subsequence of `seq`. Patience sorting with parent links:
 * `tails[k]` is where the smallest tail of a length-(k+1) run sits, and `prev` remembers what
 * each element extended, so the run can be read back at the end.
 */
function longestRun(seq: number[]): number[] {
  const tails: number[] = [];
  const prev = new Int32Array(seq.length).fill(-1);
  for (let i = 0; i < seq.length; i++) {
    let lo = 0;
    let hi = tails.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (seq[tails[mid]] < seq[i]) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0) prev[i] = tails[lo - 1];
    tails[lo] = i;
  }
  const out: number[] = [];
  for (let k = tails.length ? tails[tails.length - 1] : -1; k >= 0; k = prev[k]) out.push(k);
  return out.reverse();
}

/** The nearest earlier entry of `keep` for every position, and the nearest later one. */
function neighbours(order: string[], keep: Set<string>) {
  const before: (string | undefined)[] = new Array(order.length);
  const after: (string | undefined)[] = new Array(order.length);
  let last: string | undefined;
  for (let i = 0; i < order.length; i++) {
    before[i] = last;
    if (keep.has(order[i])) last = order[i];
  }
  last = undefined;
  for (let i = order.length - 1; i >= 0; i--) {
    after[i] = last;
    if (keep.has(order[i])) last = order[i];
  }
  return { before, after };
}

export function analyseMoves(
  current: string[],
  proposed: string[],
  phaseNow: (uid: string) => Phase,
  phaseNext: (uid: string) => Phase,
  reasonOf: (uid: string) => string
): MoveReport {
  const to = new Map(proposed.map((u, i) => [u, i]));
  // Defensive: compare only what both orders hold, so a list that changed under us cannot throw.
  const from = current.filter((u) => to.has(u));
  const fromIndex = new Map(from.map((u, i) => [u, i]));
  const stays = new Set<string>();
  for (const i of longestRun(from.map((u) => to.get(u)!))) stays.add(from[i]);

  const inProposed = neighbours(proposed, stays);
  const inCurrent = neighbours(from, stays);
  const relocations: Relocation[] = [];
  for (let i = 0; i < proposed.length; i++) {
    const uid = proposed[i];
    if (stays.has(uid) || !fromIndex.has(uid)) continue;
    relocations.push({
      uid,
      from: fromIndex.get(uid)!,
      to: i,
      fromPhase: phaseNow(uid),
      toPhase: phaseNext(uid),
      afterUid: inProposed.before[i],
      beforeUid: inProposed.after[i],
      wasAfterUid: inCurrent.before[fromIndex.get(uid)!],
      reason: reasonOf(uid)
    });
  }
  let drift = 0;
  for (const uid of stays) if (fromIndex.get(uid) !== to.get(uid)) drift++;
  return {
    relocations,
    drift,
    crossPhase: relocations.filter((r) => r.fromPhase !== r.toPhase).length,
    fromPhases: [...new Set(relocations.map((r) => r.fromPhase))],
    toPhases: [...new Set(relocations.map((r) => r.toPhase))]
  };
}
