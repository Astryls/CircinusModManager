// Browser-only stand-in for the Rust backend: lets `npm run dev` show the UI with example data.
// Nothing here ships in the Tauri build path (api.ts only imports it outside Tauri).

import type { Issue, ModInfo, Phase, Placement, Rule, Snapshot, SortResult, Source, UserData, Weight, Settings } from "./types";
import { PHASES } from "./types";

type Seed = [name: string, author: string, pkg: string, pfid: string | null, src: Source, phase: Phase, group: string, ver: string[], size: number, flags?: string];

const SEED: Seed[] = [
  ["RimWorld", "Ludeon Studios", "ludeon.rimworld", null, "ludeon", "core", "core", ["1.6"], 1.9e9, "defs"],
  ["Royalty", "Ludeon Studios", "ludeon.rimworld.royalty", null, "ludeon", "core", "core", ["1.6"], 168e6, "defs"],
  ["Ideology", "Ludeon Studios", "ludeon.rimworld.ideology", null, "ludeon", "core", "core", ["1.6"], 203e6, "defs"],
  ["Biotech", "Ludeon Studios", "ludeon.rimworld.biotech", null, "ludeon", "core", "core", ["1.6"], 241e6, "defs"],
  ["Anomaly", "Ludeon Studios", "ludeon.rimworld.anomaly", null, "ludeon", "core", "core", ["1.6"], 312e6, "defs"],
  ["Odyssey", "Ludeon Studios", "ludeon.rimworld.odyssey", null, "ludeon", "core", "core", ["1.6"], 298e6, "defs"],
  ["Prepatcher", "Zetrith", "zetrith.prepatcher", "2934420800", "workshop", "prepatch", "core", ["1.5", "1.6"], 1.2e6, "cs"],
  ["Harmony", "Brrainz", "brrainz.harmony", "2009463077", "workshop", "prepatch", "core", ["1.5", "1.6"], 0.9e6, "cs"],
  ["Visual Exceptions", "Brrainz", "brrainz.visualexceptions", "2538411704", "workshop", "prepatch", "core", ["1.5", "1.6"], 0.4e6, "cs"],
  ["HugsLib", "UnlimitedHugs", "unlimitedhugs.hugslib", "818773962", "workshop", "framework", "frameworks", ["1.5", "1.6"], 2.1e6, "cs"],
  ["Vanilla Expanded Framework", "Oskar Potocki, Taranchuk", "oskarpotocki.vanillafactionsexpanded.core", "2023507013", "workshop", "framework", "frameworks", ["1.5", "1.6"], 48e6, "cs"],
  ["Vehicle Framework", "SmashPhil", "smashphil.vehicleframework", "3014915404", "workshop", "framework", "frameworks", ["1.5", "1.6"], 31e6, "cs"],
  ["XML Extensions", "Imranfish", "imranfish.xmlextensions", "2574315206", "workshop", "framework", "frameworks", ["1.5", "1.6"], 0.7e6, "cs"],
  ["Adaptive Storage Framework", "bs, Wiri", "adaptive.storage.framework", "3033901359", "workshop", "framework", "frameworks", ["1.5", "1.6"], 3.4e6, "cs"],
  ["Vanilla Weapons Expanded", "Oskar Potocki", "vanillaexpanded.vwe", "1814383360", "workshop", "content", "qol", ["1.5", "1.6"], 22e6, "defs"],
  ["Vanilla Furniture Expanded", "Oskar Potocki", "vanillaexpanded.vfecore", "1718190143", "workshop", "content", "qol", ["1.5", "1.6"], 19e6, "defs"],
  ["Dubs Bad Hygiene", "Dubwise", "dubwise.dubsbadhygiene", "836308268", "workshop", "content", "qol", ["1.5", "1.6"], 27e6, "cs"],
  ["Rimatomics", "Dubwise", "dubwise.rimatomics", "1127530465", "workshop", "content", "qol", ["1.5", "1.6"], 58e6, "cs"],
  ["Hospitality", "Orion", "orion.hospitality", "753498552", "workshop", "content", "qol", ["1.5", "1.6"], 4.8e6, "cs"],
  ["Alpha Animals", "Sarg Bjornson", "sarg.alphaanimals", "1541721856", "workshop", "content", "qol", ["1.5", "1.6"], 113e6, "defs"],
  ["Alpha Biomes", "Sarg Bjornson", "sarg.alphabiomes", "1841354677", "workshop", "content", "qol", ["1.5", "1.6"], 96e6, "defs"],
  ["EdB Prepare Carefully", "EdB", "edbmods.edbpreparecarefully", "735106432", "workshop", "content", "qol", ["1.5", "1.6"], 1.6e6, "cs"],
  ["Character Editor", "VOID", "void.charactereditor", "1554146052", "local", "content", "qol", ["1.5", "1.6"], 2.9e6, "cs"],
  ["Pick Up And Haul", "Mehni", "mehni.pickupandhaul", "1279012058", "workshop", "content", "qol", ["1.5", "1.6"], 0.6e6, "cs"],
  ["Allow Tool", "UnlimitedHugs", "unlimitedhugs.allowtool", "761421485", "workshop", "content", "qol", ["1.5", "1.6"], 0.8e6, "cs"],
  ["Replace Stuff", "Uuugggg", "uuugggg.replacestuff", "1372003680", "workshop", "content", "qol", ["1.5", "1.6"], 0.5e6, "cs"],
  ["Interaction Bubbles", "Jaxe", "jaxe.bubbles", "1516158345", "workshop", "content", "qol", ["1.5", "1.6"], 0.3e6, "cs"],
  ["RimHUD", "Jaxe", "jaxe.rimhud", "1508850027", "workshop", "content", "qol", ["1.5", "1.6"], 0.9e6, "cs"],
  ["Better Pawn Control", "VouLT", "voult.betterpawncontrol", "1541460369", "workshop", "content", "qol", ["1.5", "1.6"], 1.1e6, "cs"],
  ["Combat Extended", "CE Team", "ceteam.combatextended", "2890901044", "steamcmd", "content", "qol", ["1.5", "1.6"], 141e6, "cs"],
  ["RIMMSqol", "Razuhl", "razuhl.rimmsqol", "1084452457", "workshop", "content", "qol", ["1.5", "1.6"], 3.2e6, "cs"],
  ["Camera+", "Brrainz", "brrainz.cameraplus", "867467808", "workshop", "content", "visual", ["1.5", "1.6"], 0.4e6, "cs"],
  ["Dubs Mint Menus", "Dubwise", "dubwise.dubsmintmenus", "1446523594", "workshop", "content", "qol", ["1.5", "1.6"], 1.7e6, "cs"],
  ["Alpha Animals — CE Patch", "Community", "community.alphaanimals.ce", "2979912040", "workshop", "patch", "qol", ["1.5", "1.6"], 0.2e6, "xml"],
  ["Bad Hygiene × VFE Patch", "Community", "community.dbh.vfe", "2814471152", "workshop", "patch", "qol", ["1.6"], 0.1e6, "xml"],
  ["Hospitality — Casino Patch", "Community", "community.hospitality.casino", "2881264430", "git", "patch", "qol", ["1.5"], 0.1e6, "xml"],
  ["Vanilla Textures Expanded", "Oskar Potocki", "vanillaexpanded.vtexe", "2032492919", "workshop", "texture", "visual", ["1.5", "1.6"], 168e6, "tex"],
  ["Vanilla Textures Expanded — Variations", "Oskar Potocki", "vanillaexpanded.vtexe.variations", "2035869776", "workshop", "texture", "visual", ["1.5", "1.6"], 41e6, "tex"],
  ["Retro Wall Textures", "Nyx", "nyx.retrowalls", "3187722041", "workshop", "texture", "visual", ["1.5", "1.6"], 6e6, "tex"],
  ["Pawn Textures Redux", "Kaeri", "kaeri.pawntexturesredux", "2760111925", "workshop", "texture", "visual", ["1.4", "1.5"], 88e6, "tex"],
  ["Performance Fish", "bs", "bs.performance", "3105420219", "workshop", "optimization", "performance", ["1.5", "1.6"], 2.4e6, "cs"],
  ["Dubs Performance Analyzer", "Dubwise", "dubwise.dubsperformanceanalyzer", "2222707451", "workshop", "optimization", "performance", ["1.5", "1.6"], 1.9e6, "cs"],
  ["RocketMan", "Krkr", "krkr.rocketman", "2479389928", "workshop", "optimization", "performance", ["1.5", "1.6"], 3.1e6, "cs"],
  ["Graphics Settings+", "Telardo", "telardo.graphicssettings", "1541713450", "workshop", "optimization", "performance", ["1.5", "1.6"], 0.6e6, "cs"],
  // inactive
  ["Vanilla Psycasts Expanded", "Oskar Potocki", "vanillaexpanded.vpsycastse", "2842502659", "workshop", "content", "qol", ["1.5", "1.6"], 61e6, "cs off"],
  ["Save Our Ship 2", "Kentington", "kentington.saveourship2", "1909914131", "workshop", "content", "qol", ["1.5"], 214e6, "cs off"],
  ["Rimefeller", "Dubwise", "dubwise.rimefeller", "1321849735", "workshop", "content", "qol", ["1.5", "1.6"], 33e6, "cs off"],
  ["Medieval Overhaul", "SirMashedPotato", "dankpyon.medieval.overhaul", "2553700067", "workshop", "content", "qol", ["1.5", "1.6"], 402e6, "defs off"],
  ["RuntimeGC", "user19990313", "user19990313.runtimegc", "962732083", "local", "optimization", "performance", ["1.4"], 0.4e6, "cs off"],
  ["Realistic Rooms Rewritten", "Lucifer", "lucifer.realisticroomsrewritten", "2559782534", "steamcmd", "content", "qol", ["1.5", "1.6"], 0.2e6, "cs off"],
  ["Ancient Urban Ruins", "MO", "mo.ancienturbanruins", "3208227718", "workshop", "content", "qol", ["1.5", "1.6"], 87e6, "defs off"]
];

function mod(s: Seed): ModInfo {
  const [name, author, pkg, pfid, src, , , ver, size, flags = ""] = s;
  const uid = `C:\\Mods\\${pkg}`;
  const cs = flags.includes("cs");
  const tex = flags.includes("tex");
  const xml = flags.includes("xml");
  return {
    uid,
    path: uid,
    packageId: pkg,
    name,
    authors: author.split(",").map((a) => a.trim()),
    description: `${name} — example description. Real descriptions come from About.xml.`,
    supportedVersions: ver,
    publishedFileId: pfid ? Number(pfid) : undefined,
    source: src,
    rules: { loadAfter: src === "ludeon" ? [] : ["brrainz.harmony"], loadBefore: [], forceLoadAfter: [], forceLoadBefore: [], incompatibleWith: [], dependencies: src === "ludeon" ? [] : [{ packageId: "brrainz.harmony", displayName: "Harmony" }] },
    contents: { assemblies: cs ? 2 : 0, patches: xml ? 3 : 0, defs: flags.includes("defs") || (!cs && !tex && !xml) ? 40 : cs ? 6 : 0, textures: tex ? 300 : 12, dds: 0, sounds: 0, languages: 1, bundlesHarmony: false, sizeBytes: size },
    modified: 1_756_000_000,
    kind: src === "ludeon" ? "official" : cs ? "code" : tex ? "textures" : "xml"
  };
}

const mods: ModInfo[] = SEED.map(mod);
const uidOf = (pkg: string) => mods.find((m) => m.packageId === pkg)!.uid;
let active: string[] = SEED.filter((s) => !(s[9] ?? "").includes("off")).map((s) => uidOf(s[2]));
const phaseOfSeed: Record<string, Phase> = Object.fromEntries(SEED.map((s) => [uidOf(s[2]), s[5]]));
let dirty = false;

let user: UserData = {
  groups: [
    { id: "core", name: "Core", color: "blue" },
    { id: "frameworks", name: "Frameworks", color: "teal" },
    { id: "qol", name: "Quality of life", color: "green" },
    { id: "visual", name: "Visual", color: "amber" },
    { id: "performance", name: "Performance", color: "coral", phase: "optimization" }
  ],
  modGroups: Object.fromEntries(SEED.map((s) => [uidOf(s[2]), s[6]])),
  pinned: [],
  phaseOverrides: {},
  notes: {},
  muted: []
};

let settings: Settings = {
  locations: { gameDir: "C:\\Program Files (x86)\\Steam\\steamapps\\common\\RimWorld", configDir: "C:\\Users\\Astryl\\AppData\\LocalLow\\Ludeon Studios\\RimWorld by Ludeon Studios\\Config", localModsDir: "C:\\Program Files (x86)\\Steam\\steamapps\\common\\RimWorld\\Mods", workshopDir: "C:\\Program Files (x86)\\Steam\\steamapps\\workshop\\content\\294100" },
  dbSources: [
    { id: "community", label: "Community rules (RimSort)", url: "https://raw.githubusercontent.com/RimSort/Community-Rules-Database/main/communityRules.json", file: "communityRules.json", enabled: true },
    { id: "steam", label: "Steam Workshop database (RimSort)", url: "https://raw.githubusercontent.com/RimSort/Steam-Workshop-Database/main/steamDB.json", file: "steamDB.json", enabled: true },
    { id: "replacements", label: "Use This Instead (emipa606, MIT)", url: "https://raw.githubusercontent.com/emipa606/UseThisInstead/main/replacements.json.gz", file: "replacements.json.gz", enabled: true },
    { id: "noversion", label: "No Version Warning (emipa606, MIT)", url: "https://raw.githubusercontent.com/emipa606/NoVersionWarning/main/{version}/ModIdsToFix.xml", file: "ModIdsToFix.xml", enabled: true }
  ],
  showWeight: true,
  includeLocalRuns: true,
  alphabeticalWithinPhase: false,
  updateDatabasesOnStart: false
};

const rules: Rule[] = [
  { kind: "loadAfter", subject: "voult.betterpawncontrol", target: "oskarpotocki.vanillafactionsexpanded.core", source: "community", comment: "BPC patches VEF work tabs" },
  { kind: "loadBefore", subject: "voult.betterpawncontrol", target: "krkr.rocketman", source: "community" },
  { kind: "loadAfter", subject: "voult.betterpawncontrol", target: "dubwise.dubsbadhygiene", source: "user", comment: "my own — BPC bathing policy" },
  { kind: "incompatible", subject: "bs.performance", target: "razuhl.rimmsqol", source: "about" },
  { kind: "loadBottom", subject: "krkr.rocketman", source: "community", comment: "RocketMan must load last" },
  ...mods.filter((m) => m.source !== "ludeon").map((m): Rule => ({ kind: "loadAfter", subject: m.packageId, target: "brrainz.harmony", source: "about" }))
];

const weightSeed: Record<string, [number, boolean]> = {
  "krkr.rocketman": [3.4, true], "bs.performance": [0.3, true], "oskarpotocki.vanillafactionsexpanded.core": [6.2, true], "dubwise.dubsbadhygiene": [4.1, true], "sarg.alphaanimals": [1.4, true],
  "ceteam.combatextended": [17.8, true], "jaxe.rimhud": [2.6, true], "orion.hospitality": [1.1, true], "smashphil.vehicleframework": [7.9, true], "unlimitedhugs.hugslib": [0.2, true],
  "brrainz.harmony": [0.1, true], "dubwise.rimatomics": [2.2, true], "voult.betterpawncontrol": [0.6, true], "razuhl.rimmsqol": [3.0, false], "jaxe.bubbles": [0.4, true], "mehni.pickupandhaul": [1.9, true]
};
const weights: Record<string, Weight> = Object.fromEntries(
  Object.entries(weightSeed).map(([pkg, [share, ranked]]) => {
    const band = !ranked ? "insufficient" : share < 0.5 ? "negligible" : share <= 2 ? "light" : share <= 5 ? "moderate" : share <= 15 ? "heavy" : "veryheavy";
    return [pkg, { packageId: pkg, share, band, ranked, seen: 300, measured: 240, rankedRuns: ranked ? 200 : 12, installs: ranked ? 48 : 3, netLow: null, netHigh: null, withheld: false, origin: "api" } satisfies Weight];
  })
);

function placements(order: string[]): Placement[] {
  const reason: Record<Phase, string> = { core: "Official content, pinned by RimWorld", prepatch: "Patches the game before other mods load", framework: "Known framework", content: "Adds content", patch: "XML patches only; must see its targets first", texture: "Textures only", optimization: "Optimizes other mods, so it must see them all" };
  return order.map((uid) => {
    const phase = user.phaseOverrides[uid] ?? user.groups.find((g) => g.id === user.modGroups[uid])?.phase ?? phaseOfSeed[uid] ?? "content";
    return { uid, phase, reason: user.phaseOverrides[uid] ? "Set by you" : reason[phase] };
  });
}

function issues(order: string[]): Issue[] {
  const idx = (pkg: string) => order.indexOf(uidOf(pkg));
  const out: Issue[] = [];
  const has = (pkg: string) => idx(pkg) >= 0;
  if (has("voult.betterpawncontrol") && has("oskarpotocki.vanillafactionsexpanded.core") && idx("voult.betterpawncontrol") < idx("oskarpotocki.vanillafactionsexpanded.core"))
    out.push({ kind: "orderViolation", uid: uidOf("voult.betterpawncontrol"), targetUid: uidOf("oskarpotocki.vanillafactionsexpanded.core"), rule: "loadAfter", source: "community", comment: "BPC patches VEF work tabs" });
  if (has("bs.performance") && has("razuhl.rimmsqol")) out.push({ kind: "incompatible", uid: uidOf("razuhl.rimmsqol"), otherUid: uidOf("bs.performance"), source: "about" });
  for (const m of mods) if (order.includes(m.uid) && m.source !== "ludeon" && !m.supportedVersions.includes("1.6")) out.push({ kind: "versionMismatch", uid: m.uid, supported: m.supportedVersions });
  if (has("krkr.rocketman")) {
    const after = order.slice(idx("krkr.rocketman") + 1).filter((u) => phaseOfSeed[u] !== "optimization");
    if (after.length) out.push({ kind: "misplacedOptimization", uid: uidOf("krkr.rocketman"), afterUids: after });
  }
  if (has("vanillaexpanded.vtexe") && has("nyx.retrowalls")) {
    const a = uidOf("vanillaexpanded.vtexe"), b = uidOf("nyx.retrowalls");
    const uids = idx("vanillaexpanded.vtexe") < idx("nyx.retrowalls") ? [a, b] : [b, a];
    for (const p of ["things/building/linked/wall_atlas", "things/building/linked/wallsmooth_atlas", "things/building/linked/wallbricks_atlas", "things/building/door/door_mover"]) out.push({ kind: "textureCollision", path: p, uids, winnerUid: uids[1] });
  }
  return out;
}

function snapshot(): Snapshot {
  return {
    locations: settings.locations,
    gameVersion: { full: "1.6.4530 rev1235", majorMinor: "1.6" },
    mods,
    active,
    missing: ["some.missing.mod"],
    issues: issues(active),
    placements: placements(active),
    rules,
    user,
    settings,
    weights,
    weightsFetchedAt: 1_757_000_000,
    dirty,
    dbLoaded: ["communityRules.json (7,412 rules)", "steamDB.json (31,988 items)"],
    scannedAt: 1_757_000_000,
    inspecting: 0,
    issuesTruncated: 0,
    updates: [{ uid: uidOf("krkr.rocketman"), publishedFileId: 2479389928, name: "RocketMan", localModified: 1_750_000_000, remoteUpdated: 1_756_500_000, source: "workshop" }],
    updatesCheckedAt: 1_757_000_000
  };
}

function haloSort(): SortResult {
  const rank = (uid: string) => PHASES.findIndex((p) => p.id === (placements([uid])[0].phase));
  const order = [...active].map((uid, i) => ({ uid, i })).sort((a, b) => rank(a.uid) - rank(b.uid) || a.i - b.i).map((x) => x.uid);
  // honour the two community rules in the seed
  const bpc = uidOf("voult.betterpawncontrol"), vef = uidOf("oskarpotocki.vanillafactionsexpanded.core");
  if (order.indexOf(bpc) < order.indexOf(vef)) { order.splice(order.indexOf(bpc), 1); order.splice(order.indexOf(vef) + 1, 0, bpc); }
  const rm = uidOf("krkr.rocketman");
  if (order.includes(rm)) { order.splice(order.indexOf(rm), 1); order.push(rm); }
  const moves: [string, number, number][] = order.map((uid, to) => [uid, active.indexOf(uid), to] as [string, number, number]).filter(([, from, to]) => from !== to);
  return { order, placements: placements(order), moves, issues: issues(order) };
}

const queue = {
  items: [
    { id: 2009463077, name: "Harmony", status: "done", attempts: 1, bytes: 935611, path: "C:\\RimWorld\\Mods\\2009463077", addedAt: 1, finishedAt: 2 },
    { id: 818773962, name: "HugsLib", status: "downloading", attempts: 0, addedAt: 1 },
    { id: 2023507013, name: "Vanilla Expanded Framework", status: "queued", attempts: 1, error: "Failure", addedAt: 1 },
    { id: 3014915404, name: "Vehicle Framework", status: "queued", attempts: 0, addedAt: 1 },
    { id: 1541460369, name: "Better Pawn Control", status: "failed", attempts: 4, error: "Timeout", addedAt: 1, finishedAt: 3 }
  ],
  throttle: { batchSize: 12, cleanStreak: 0, level: 1, cooldownUntil: null, last: { requested: 25, succeeded: 20, failed: 5, timedOut: 0, authFailed: false, stalled: false, seconds: 84 } },
  paused: false, currentBatch: [818773962, 2023507013, 3014915404], currentItem: 818773962, running: true, steamcmdInstalled: true, installing: false,
  log: ["Batch of 3 (batch size 12)", "Loading Steam API...OK", "Connecting anonymously to Steam Public...OK", "Waiting for client config...OK", "Downloading item 818773962 ..."]
} as const;

export async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  await new Promise((r) => setTimeout(r, 30));
  const A = args as Record<string, any>;
  // Load testing: a page may provide a full backend snapshot (see src-tauri/examples/dump_snapshot.rs).
  const fixture = (globalThis as any).__CIRCINUS_FIXTURE__ as Snapshot | undefined;
  switch (cmd) {
    case "get_snapshot":
    case "rescan":
      return (fixture ?? snapshot()) as T;
    case "set_active":
      active = (A.uids as string[]).filter((u, i, arr) => arr.indexOf(u) === i && mods.some((m) => m.uid === u));
      dirty = true;
      return snapshot() as T;
    case "activate": {
      const add = (A.uids as string[]).filter((u) => !active.includes(u));
      const at = A.at as number | null;
      if (at == null || at > active.length) active = [...active, ...add];
      else active = [...active.slice(0, at), ...add, ...active.slice(at)];
      dirty = true;
      return snapshot() as T;
    }
    case "deactivate":
      active = active.filter((u) => !(A.uids as string[]).includes(u));
      dirty = true;
      return snapshot() as T;
    case "halo": {
      const r = haloSort();
      if (A.apply) { active = r.order; dirty = true; }
      return r as T;
    }
    case "validate":
      return issues(active) as T;
    case "save_mods_config":
      dirty = false;
      return settings.locations.configDir + "\\ModsConfig.xml" as T;
    case "import_list": {
      const text = (A.text as string) ?? "";
      const ids = Array.from(text.matchAll(/\[([a-z0-9._-]+\.[a-z0-9._-]+)\]/gi)).map((m) => m[1].toLowerCase());
      const list = ids.length ? ids : text.split(/\r?\n/).map((l) => l.trim().toLowerCase()).filter((l) => l.includes("."));
      const uids = list.map((id) => mods.find((m) => m.packageId === id)?.uid).filter(Boolean) as string[];
      const missing = list.filter((id) => !mods.some((m) => m.packageId === id));
      return { list: { packageIds: list, format: "Text list" }, uids, missing } as T;
    }
    case "apply_import":
      if (A.append) active = [...active, ...(A.uids as string[]).filter((u) => !active.includes(u))];
      else active = A.uids as string[];
      dirty = true;
      return snapshot() as T;
    case "update_settings":
      settings = A.settings as Settings;
      return snapshot() as T;
    case "autodetect_locations":
      return settings.locations as T;
    case "update_user":
      user = A.user as UserData;
      return snapshot() as T;
    case "update_databases":
      return ["Community rules (RimSort): updated", "Steam Workshop database (RimSort): already current", "Use This Instead (emipa606, MIT): updated", "No Version Warning (emipa606, MIT): updated"] as T;
    case "refresh_weights":
      return Object.keys(weights).length as T;
    case "refresh_local_weights":
      return 0 as T;
    case "get_files":
      return { textures: [], patches: [], defs: [], assemblies: [] } as T;
    case "get_user_rules":
      return { timestamp: 0, rules: rules.filter((r) => r.source === "user"), ignore: [] } as T;
    case "edit_user_rule":
      return snapshot() as T;
    case "get_description":
      return (mods.find((m) => m.uid === A.uid)?.description ?? "") as T;
    case "downloads_state":
    case "downloads_remove":
    case "downloads_clear_finished":
    case "downloads_pause":
      return structuredClone(queue) as unknown as T;
    case "downloads_add":
    case "downloads_add_text":
      return { added: 2, skipped: [[1, "not a RimWorld workshop item"]] } as T;
    case "downloads_add_missing":
      return [{ added: 1, skipped: [] }, ["some.missing.mod"]] as T;
    case "downloads_retry_failed":
      return 1 as T;
    case "steamcmd_install":
      return undefined as T;
    case "import_collection":
      return { ids: [2009463077, 818773962, 999], installed: [[2009463077, uidOf("brrainz.harmony")], [818773962, uidOf("unlimitedhugs.hugslib")]], missing: [999], names: { "2009463077": "Harmony", "818773962": "HugsLib", "999": "Some Missing Mod" } } as T;
    case "import_rentry":
      return { preview: { list: { packageIds: ["brrainz.harmony", "no.such"], format: "Rentry list" }, uids: [uidOf("brrainz.harmony")], missing: ["no.such"] }, missingWorkshopIds: [999] } as T;
    case "check_updates":
      return 1 as T;
    case "app_data_dir":
      return "C:\\Users\\Astryl\\AppData\\Local\\Circinus" as T;
    default:
      throw new Error(`mock: unknown command ${cmd}`);
  }
}
