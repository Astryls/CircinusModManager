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

export interface Settings {
  locations: Locations;
  dbSources: DbSource[];
  showWeight: boolean;
  includeLocalRuns: boolean;
  alphabeticalWithinPhase: boolean;
  updateDatabasesOnStart: boolean;
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
