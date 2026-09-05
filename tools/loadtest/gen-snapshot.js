// Node-side generator of a backend-shaped snapshot at a 2,000-mod scale, for the UI load test.
const N = 1990, ACTIVE = 1081;
let seed = 7; const rnd = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
const pick = (a) => a[Math.floor(rnd() * a.length)];
const words = ["Vanilla","Expanded","Better","Simple","Realistic","Medieval","Alpha","Rim","Combat","Textures","Framework","Patch","Core","Retexture","Animals","Weapons","Furniture","Psycasts","Performance","Optimizer","Ruins","Ships","Faces","Hair","Apparel","Genes"];
const mods = [], uids = [];
for (let i = 0; i < N; i++) {
  const name = `${pick(words)} ${pick(words)} ${i % 97 === 0 ? "— Reforged (1.6) & More <Fixed>" : ""}`.trim();
  const pkg = i === 0 ? "ludeon.rimworld" : `author${i % 40}.${name.toLowerCase().replace(/[^a-z0-9]/g, "")}${i}`;
  const uid = i % 5 ? `C:\\Program Files (x86)\\Steam\\steamapps\\workshop\\content\\294100\\${700000000 + i * 1234}` : `C:\\Program Files (x86)\\Steam\\steamapps\\common\\RimWorld\\Mods\\${name.replace(/ /g, "_")}_${i}`;
  uids.push(uid);
  mods.push({ uid, path: uid, packageId: i === 3 ? "" : pkg, name, authors: [`Author ${i % 40}`], description: "", supportedVersions: pick([["1.5","1.6"],["1.6"],["1.4","1.5"],[]]),
    publishedFileId: i % 5 ? 700000000 + i * 1234 : undefined, source: i === 0 ? "ludeon" : i % 5 ? "workshop" : "local",
    rules: { loadAfter: [], loadBefore: [], forceLoadAfter: [], forceLoadBefore: [], incompatibleWith: [], dependencies: [] },
    contents: { assemblies: i % 3, patches: i % 7 === 3 ? 2 : 0, defs: 5, textures: i % 7 === 4 ? 80 : 0, dds: 0, sounds: 0, languages: 0, bundlesHarmony: false, sizeBytes: 1e6 * (i % 50) },
    modified: 1_756_000_000, kind: i === 0 ? "official" : i % 3 ? "code" : "xml", invalid: i === 1500 ? "No About/About.xml in this folder" : undefined });
}
const active = uids.slice(0, ACTIVE);
const issues = [];
for (let i = 0; i < 2600; i++) { const a = active[i % ACTIVE], b = active[(i * 7 + 3) % ACTIVE]; issues.push({ kind: "textureCollision", path: `things/building/item${i}`, uids: [a, b], winnerUid: b }); }
for (let i = 0; i < 300; i++) issues.push({ kind: "orderViolation", uid: active[i * 3], targetUid: active[i * 3 + 1], rule: "loadAfter", source: "community", comment: "synthetic" });
for (let i = 0; i < 80; i++) issues.push({ kind: "missingDependency", uid: active[i], dependency: "some.dep", displayName: "Some Dep", workshopUrl: "https://x" });
for (let i = 0; i < 20; i++) issues.push({ kind: "incompatible", uid: active[i * 5], otherUid: active[i * 5 + 2], source: "about" });
issues.push({ kind: "cycle", uids: [active[1], active[2], active[3]], chain: "A → B → C → A", rules: [] });
for (let i = 0; i < 30; i++) issues.push({ kind: "versionMismatch", uid: active[i + 100], supported: ["1.4"] });
issues.push({ kind: "misplacedOptimization", uid: active[500], afterUids: active.slice(501, 600) });
issues.push({ kind: "duplicatePackageId", packageId: "author1.x", uids: [uids[10], uids[11]] });
issues.push({ kind: "missingPackageId", uid: uids[3] });
issues.push({ kind: "invalid", uid: uids[1500], reason: "No About/About.xml" });
const rules = [];
for (let i = 0; i < 5550; i++) rules.push({ kind: i % 9 === 0 ? "loadBefore" : "loadAfter", subject: mods[i % N].packageId, target: mods[(i * 13) % N].packageId, source: pick(["about", "community", "user"]) });
const phases = ["core","prepatch","framework","content","patch","texture","optimization"];
const placements = active.map((uid, i) => ({ uid, phase: i === 0 ? "core" : phases[(i % 7)], reason: "synthetic" }));
const snap = { locations: { gameDir: "C:\\RimWorld", configDir: "C:\\Config", localModsDir: "C:\\RimWorld\\Mods", workshopDir: "C:\\ws" }, gameVersion: { full: "1.6.4530 rev1235", majorMinor: "1.6" }, mods, active, missing: ["no.such.mod"], issues, placements, rules,
  user: { groups: [{ id: "core", name: "Core", color: "blue" }, { id: "performance", name: "Performance", color: "coral", phase: "optimization" }], modGroups: {}, pinned: [], phaseOverrides: {}, notes: {}, muted: [] },
  settings: { locations: {}, dbSources: [], showWeight: false, includeLocalRuns: true, alphabeticalWithinPhase: false, updateDatabasesOnStart: false },
  weights: {}, weightsFetchedAt: 0, dirty: false, dbLoaded: ["communityRules.json"], scannedAt: 1, inspecting: 0, issuesTruncated: 0 };
require("fs").writeFileSync("tools/loadtest/snapshot-js.json", JSON.stringify(snap));
console.log("bytes", JSON.stringify(snap).length);
