// Mirrors crates/circinus-core/src/model.rs and src-tauri/src/state.rs (camelCase over the wire).

export type Source = "ludeon" | "workshop" | "local" | "steamcmd" | "git";
export type ModKind = "unknown" | "official" | "code" | "xml" | "textures" | "translation" | "scenario";
export type Phase = "core" | "prepatch" | "framework" | "content" | "patch" | "texture" | "optimization";
export type RuleSource = "about" | "manifest" | "community" | "user" | "halo";
export type RuleKind = "loadAfter" | "loadBefore" | "incompatible" | "loadTop" | "loadBottom";
export type Severity = "error" | "warning" | "note";
export type Band = "negligible" | "light" | "moderate" | "heavy" | "veryheavy" | "insufficient" | "unknown";

export interface Dependency {
  packageId: string;
  displayName?: string;
  workshopUrl?: string;
  downloadUrl?: string;
  alternatives?: string[];
}

export interface AboutRules {
  loadAfter: string[];
  loadBefore: string[];
  forceLoadAfter: string[];
  forceLoadBefore: string[];
  incompatibleWith: string[];
  dependencies: Dependency[];
}

export interface Contents {
  assemblies: number;
  patches: number;
  defs: number;
  textures: number;
  dds: number;
  sounds: number;
  languages: number;
  bundlesHarmony: boolean;
  sizeBytes: number;
}

export interface ModInfo {
  uid: string;
  path: string;
  packageId: string;
  name: string;
  authors: string[];
  description: string;
  supportedVersions: string[];
  modVersion?: string;
  url?: string;
  steamAppId?: number;
  publishedFileId?: number;
  source: Source;
  preview?: string;
  rules: AboutRules;
  manifest?: unknown;
  loadFolders?: unknown;
  contents: Contents;
  modified: number;
  invalid?: string;
  kind: ModKind;
}

export interface Rule {
  kind: RuleKind;
  subject: string;
  target?: string;
  source: RuleSource;
  comment?: string;
}

export type Issue =
  | { kind: "missingDependency"; uid: string; dependency: string; displayName?: string; installedUid?: string; workshopUrl?: string }
  | { kind: "incompatible"; uid: string; otherUid: string; source: RuleSource }
  | { kind: "orderViolation"; uid: string; targetUid: string; rule: RuleKind; source: RuleSource; comment?: string }
  | { kind: "versionMismatch"; uid: string; supported: string[] }
  | { kind: "cycle"; uids: string[]; chain: string; rules: Rule[] }
  | { kind: "textureCollision"; path: string; uids: string[]; winnerUid: string }
  | { kind: "misplacedOptimization"; uid: string; afterUids: string[] }
  | { kind: "duplicatePackageId"; packageId: string; uids: string[] }
  | { kind: "missingPackageId"; uid: string }
  | { kind: "invalid"; uid: string; reason: string };

export interface Placement {
  uid: string;
  phase: Phase;
  reason: string;
}

export interface SortResult {
  order: string[];
  placements: Placement[];
  moves: [string, number, number][];
  issues: Issue[];
}

export interface Locations {
  gameDir?: string | null;
  configDir?: string | null;
  localModsDir?: string | null;
  workshopDir?: string | null;
}

export interface GameVersion {
  full: string;
  majorMinor: string;
}

export interface DbSource {
  id: string;
  label: string;
  url: string;
  file: string;
  enabled: boolean;
}

export type DdsFormat = "bc1" | "bc3" | "bc7";
export type DdsQuality = "quick" | "balanced" | "high" | "max";

export interface DdsSettings {
  /** Format for textures with alpha; opaque ones are always BC1. */
  alphaFormat: DdsFormat;
  quality: DdsQuality;
  mipmaps: boolean;
  /** 0 = all cores but one. */
  threads: number;
  /** Convert new and updated mods on their own. */
  auto: boolean;
}

export type LaunchMethod = "auto" | "steam" | "executable";

export interface LaunchSettings {
  method: LaunchMethod;
  /** Explicit executable; null = detect from the game folder. */
  executable: string | null;
  /** Extra command-line arguments, e.g. `-popupwindow`. */
  args: string;
  /** Write ModsConfig.xml first when there are unsaved changes. */
  saveFirst: boolean;
}

export interface LaunchInfo {
  executable: string | null;
  executableExists: boolean;
  steamInstall: boolean;
  autoResolvesTo: "steam" | "executable";
}

export interface Settings {
  locations: Locations;
  dbSources: DbSource[];
  showWeight: boolean;
  includeLocalRuns: boolean;
  alphabeticalWithinPhase: boolean;
  updateDatabasesOnStart: boolean;
  dds: DdsSettings;
  launch: LaunchSettings;
}

export interface Group {
  id: string;
  name: string;
  color: string;
  phase?: Phase | null;
}

export interface UserData {
  groups: Group[];
  modGroups: Record<string, string>;
  pinned: string[];
  phaseOverrides: Record<string, Phase>;
  notes: Record<string, string>;
  muted: string[];
  /** Mods whose textures must not be converted. */
  ddsExcluded: string[];
}

export interface Weight {
  packageId: string;
  share: number | null;
  band: Band;
  ranked: boolean;
  seen: number | null;
  measured: number | null;
  rankedRuns: number | null;
  installs: number | null;
  netLow: number | null;
  netHigh: number | null;
  withheld: boolean;
  origin: "api" | "local";
}

export interface Snapshot {
  locations: Locations;
  gameVersion: GameVersion;
  mods: ModInfo[];
  active: string[];
  missing: string[];
  issues: Issue[];
  placements: Placement[];
  rules: Rule[];
  user: UserData;
  settings: Settings;
  weights: Record<string, Weight>;
  weightsFetchedAt: number;
  dirty: boolean;
  dbLoaded: string[];
  scannedAt: number;
  /** Mods whose folders are still being inspected in the background. */
  inspecting: number;
  /** Texture collisions left out of `issues` to keep the payload small. */
  issuesTruncated: number;
  /** Installed workshop mods with a newer version on the Workshop (from the last check). */
  updates: UpdateInfo[];
  updatesCheckedAt: number;
  /** Mods that appeared, disappeared or changed since the previous session (or the last acknowledgement). */
  changes: ModChange[];
  /** Edits to ModsConfig.xml made outside Circinus since then. */
  listChange: ListChange | null;
  /** Unix seconds of the baseline the changes are measured from (0 = first run). */
  changesSince: number;
  /** uid → what Circinus has converted for it. */
  dds: Record<string, DdsSummary>;
}

export interface DdsSummary {
  count: number;
  ddsBytes: number;
  pngBytes: number;
  /** Bytes the GPU would hold for these textures uncompressed (RGBA8 with mips). */
  vramBefore: number;
  newest: number;
}

// ---- textures ----
export interface DdsProgress {
  total: number;
  done: number;
  converted: number;
  failed: number;
  pngBytes: number;
  ddsBytes: number;
  current: string;
}

export interface DdsReport {
  mods: number;
  converted: number;
  failed: number;
  current: number;
  shipped: number;
  pngBytes: number;
  ddsBytes: number;
  seconds: number;
  cancelled: boolean;
  reverted: number;
  bytesFreed: number;
}

export interface TexState {
  running: boolean;
  phase: "idle" | "scanning" | "converting" | "reverting";
  progress: DdsProgress;
  startedAt: number;
  finishedAt: number;
  errors: [string, string, string][];
  report: DdsReport | null;
}

export interface ModTextures {
  uid: string;
  name: string;
  active: boolean;
  pngs: number;
  dds: number;
  converted: number;
  ddsBytes: number;
  pngBytes: number;
  excluded: boolean;
}

export type ChangeKind = "added" | "removed" | "updated";
export type ChangeReason = "workshopUpdate" | "versionChange" | "filesChanged" | "renamed" | "sourceChanged";

export interface ModChange {
  kind: ChangeKind;
  uid: string;
  name: string;
  packageId: string;
  publishedFileId?: number | null;
  source: Source;
  /** In the active list (for removed mods: was in the list when the baseline was taken). */
  active: boolean;
  reasons: ChangeReason[];
  oldVersion?: string | null;
  newVersion?: string | null;
  /** Unix seconds of the change when known (Workshop update time, else the folder's mtime). */
  when: number;
}

export interface ListChange {
  added: string[];
  removed: string[];
  reordered: boolean;
}

export interface UpdateInfo {
  uid: string;
  publishedFileId: number;
  name: string;
  localModified: number;
  remoteUpdated: number;
  source: Source;
}

// ---- downloads ----
export type ItemStatus = "queued" | "downloading" | "done" | "failed" | "cancelled";

export interface QueueItem {
  id: number;
  name?: string;
  status: ItemStatus;
  attempts: number;
  error?: string;
  bytes?: number;
  path?: string;
  addedAt: number;
  finishedAt?: number;
}

export interface BatchStats {
  requested: number;
  succeeded: number;
  failed: number;
  timedOut: number;
  authFailed: boolean;
  stalled: boolean;
  seconds: number;
}

export interface Throttle {
  batchSize: number;
  cleanStreak: number;
  level: number;
  cooldownUntil: number | null;
  last: BatchStats | null;
}

export interface QueueState {
  items: QueueItem[];
  throttle: Throttle;
  paused: boolean;
  currentBatch: number[];
  currentItem: number | null;
  running: boolean;
  steamcmdInstalled: boolean;
  installing: boolean;
  log: string[];
}

export interface AddResult {
  added: number;
  skipped: [number, string][];
}

export interface SteamCmdStatus {
  installed: boolean;
  installing: boolean;
  root: string;
  exe: string;
  downloadsDir: string;
  consoleLog: string;
  consoleLogBytes: number;
  modsDir: string | null;
  workshopDir: string | null;
  queued: number;
  running: boolean;
  paused: boolean;
  batchSize: number;
  cooldownUntil: number | null;
}

export interface TestOutcome {
  loggedIn: boolean;
  lines: number;
  stalled: boolean;
  exitCode: number | null;
  seconds: number;
}

export interface CollectionPreview {
  ids: number[];
  installed: [number, string][];
  missing: number[];
  names: Record<string, string>;
}

export interface RentryPreview {
  preview: ImportPreview;
  missingWorkshopIds: number[];
}

export interface ImportedList {
  packageIds: string[];
  gameVersion?: string;
  format: string;
}

export interface ImportPreview {
  list: ImportedList;
  uids: string[];
  missing: string[];
}

export interface RulesFile {
  timestamp: number;
  rules: Rule[];
  ignore: Rule[];
}

export interface ModFiles {
  textures: string[];
  patches: string[];
  defs: string[];
  assemblies: string[];
}

export const PHASES: { id: Phase; name: string; color: string; note: string }[] = [
  { id: "core", name: "Core & DLC", color: "blue", note: "Pinned by RimWorld" },
  { id: "prepatch", name: "Prepatch & Harmony", color: "violet", note: "Runs before everything" },
  { id: "framework", name: "Frameworks", color: "teal", note: "Libraries other mods need" },
  { id: "content", name: "Content", color: "green", note: "Things, pawns, biomes, rules" },
  { id: "patch", name: "Patches & compat", color: "pink", note: "Must see their targets first" },
  { id: "texture", name: "Texture overrides", color: "amber", note: "Last of a kind wins" },
  { id: "optimization", name: "Optimization", color: "coral", note: "Always last" }
];

export const SOURCE_LABEL: Record<Source, string> = { ludeon: "Ludeon", workshop: "Steam", local: "Local", steamcmd: "SteamCMD", git: "Git" };
export const SOURCE_GLYPH: Record<Source, string> = { ludeon: "L", workshop: "S", local: "F", steamcmd: "C", git: "G" };

export const BAND_LABEL: Record<Band, string> = {
  negligible: "Negligible",
  light: "Light",
  moderate: "Moderate",
  heavy: "Heavy",
  veryheavy: "Very heavy",
  insufficient: "Few runs",
  unknown: "Not measured"
};

export const REASON_LABEL: Record<ChangeReason, string> = {
  workshopUpdate: "updated on the Workshop",
  versionChange: "new version",
  filesChanged: "files changed",
  renamed: "renamed",
  sourceChanged: "comes from a different place now"
};

/** "Updated on the Workshop 3 Sep · v1.2 → v1.3" */
export function describeChange(c: ModChange): string {
  const date = c.when ? new Date(c.when * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "";
  if (c.kind === "added") return `Installed${date ? ` ${date}` : ""}${c.newVersion ? ` · v${c.newVersion}` : ""}${c.active ? " · in your list" : ""}`;
  if (c.kind === "removed") return `No longer installed${c.active ? " · it was in your list" : ""}`;
  const parts: string[] = [];
  const main = c.reasons.find((r) => r === "workshopUpdate") ?? c.reasons[0];
  if (main) parts.push(REASON_LABEL[main].replace(/^./, (ch) => ch.toUpperCase()) + (date ? ` ${date}` : ""));
  if (c.oldVersion !== c.newVersion && (c.oldVersion || c.newVersion)) parts.push(`v${c.oldVersion ?? "?"} → v${c.newVersion ?? "?"}`);
  for (const r of c.reasons) if (r !== main && r !== "versionChange" && r !== "filesChanged") parts.push(REASON_LABEL[r]);
  return parts.join(" · ");
}

export function severityOf(i: Issue): Severity {
  switch (i.kind) {
    case "missingDependency":
    case "incompatible":
    case "cycle":
    case "invalid":
      return "error";
    case "textureCollision":
      return "note";
    default:
      return "warning";
  }
}

export function primaryUid(i: Issue): string | undefined {
  switch (i.kind) {
    case "textureCollision":
      return i.winnerUid;
    case "cycle":
    case "duplicatePackageId":
      return i.uids[0];
    default:
      return i.uid;
  }
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function initials(name: string): string {
  return name
    .replace(/[^A-Za-z0-9 ]/g, "")
    .split(" ")
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join("");
}
