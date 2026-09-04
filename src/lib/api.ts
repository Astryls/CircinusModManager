// Thin wrapper over Tauri's invoke, with a browser mock so the UI can run outside Tauri.

import type { AddResult, CollectionPreview, DdsReport, ImportPreview, Issue, LaunchInfo, Locations, ModFiles, ModTextures, QueueState, RentryPreview, Rule, RulesFile, Settings, Snapshot, SortResult, SteamCmdStatus, TestOutcome, TexState, UserData } from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

let invokeImpl: Invoke | null = null;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!invokeImpl) {
    if (inTauri) {
      const core = await import("@tauri-apps/api/core");
      invokeImpl = core.invoke as Invoke;
    } else {
      const mock = await import("./mock");
      invokeImpl = mock.invoke as Invoke;
    }
  }
  return invokeImpl<T>(cmd, args);
}

export const api = {
  snapshot: () => invoke<Snapshot>("get_snapshot"),
  description: (uid: string) => invoke<string>("get_description", { uid }),
  rescan: (full = false) => invoke<Snapshot>("rescan", { full }),
  setActive: (uids: string[]) => invoke<Snapshot>("set_active", { uids }),
  activate: (uids: string[], at?: number) => invoke<Snapshot>("activate", { uids, at: at ?? null }),
  deactivate: (uids: string[]) => invoke<Snapshot>("deactivate", { uids }),
  halo: (apply: boolean) => invoke<SortResult>("halo", { apply }),
  validate: () => invoke<Issue[]>("validate"),
  save: () => invoke<string>("save_mods_config"),
  importList: (path?: string, text?: string) => invoke<ImportPreview>("import_list", { path: path ?? null, text: text ?? null }),
  applyImport: (uids: string[], append: boolean) => invoke<Snapshot>("apply_import", { uids, append }),
  updateSettings: (settings: Settings) => invoke<Snapshot>("update_settings", { settings }),
  autodetect: () => invoke<Locations>("autodetect_locations"),
  updateUser: (user: UserData) => invoke<Snapshot>("update_user", { user }),
  updateDatabases: () => invoke<string[]>("update_databases"),
  refreshWeights: () => invoke<number>("refresh_weights"),
  refreshLocalWeights: () => invoke<number>("refresh_local_weights"),
  files: (uid: string) => invoke<ModFiles>("get_files", { uid }),
  userRules: () => invoke<RulesFile>("get_user_rules"),
  editUserRule: (rule: Rule, remove: boolean) => invoke<Snapshot>("edit_user_rule", { edit: { rule, remove } }),
  appDataDir: () => invoke<string>("app_data_dir"),
  // downloads
  downloadsState: () => invoke<QueueState>("downloads_state"),
  downloadsAdd: (ids: number[]) => invoke<AddResult>("downloads_add", { ids }),
  downloadsAddText: (text: string) => invoke<AddResult>("downloads_add_text", { text }),
  downloadsRemove: (ids: number[]) => invoke<QueueState>("downloads_remove", { ids }),
  downloadsRetryFailed: () => invoke<number>("downloads_retry_failed"),
  downloadsClearFinished: () => invoke<QueueState>("downloads_clear_finished"),
  downloadsPause: (paused: boolean) => invoke<QueueState>("downloads_pause", { paused }),
  downloadsAddMissing: () => invoke<[AddResult, string[]]>("downloads_add_missing"),
  steamcmdInstall: () => invoke<void>("steamcmd_install"),
  steamcmdStatus: () => invoke<SteamCmdStatus>("steamcmd_status"),
  steamcmdTest: () => invoke<TestOutcome>("steamcmd_test"),
  acknowledgeChanges: () => invoke<Snapshot>("acknowledge_changes"),
  // launching
  launchInfo: () => invoke<LaunchInfo>("get_launch_info"),
  launchGame: () => invoke<string>("launch_game"),
  // textures
  ddsState: () => invoke<TexState>("dds_state"),
  ddsOverview: () => invoke<ModTextures[]>("dds_overview"),
  ddsStart: (uids: string[]) => invoke<void>("dds_start", { uids }),
  ddsCancel: () => invoke<void>("dds_cancel"),
  ddsRevert: (uids: string[]) => invoke<DdsReport>("dds_revert", { uids }),
  importCollection: (text: string) => invoke<CollectionPreview>("import_collection", { text }),
  importRentry: (url: string) => invoke<RentryPreview>("import_rentry", { url }),
  checkUpdates: () => invoke<number>("check_updates")
};

export async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const ev = await import("@tauri-apps/api/event");
  const un = await ev.listen<T>(event, (e) => handler(e.payload));
  return un;
}

export async function openPath(path: string) {
  if (!inTauri) return;
  const opener = await import("@tauri-apps/plugin-opener");
  await opener.openPath(path);
}

export async function revealPath(path: string) {
  if (!inTauri) return;
  const opener = await import("@tauri-apps/plugin-opener");
  await opener.revealItemInDir(path);
}

export async function openUrl(url: string) {
  if (!inTauri) {
    window.open(url, "_blank");
    return;
  }
  const opener = await import("@tauri-apps/plugin-opener");
  try {
    await opener.openUrl(url);
  } catch (e) {
    console.error(`[circinus] could not open ${url}:`, e);
    throw e;
  }
}

export async function pickFile(filters?: { name: string; extensions: string[] }[]): Promise<string | null> {
  if (!inTauri) return null;
  const dialog = await import("@tauri-apps/plugin-dialog");
  const r = await dialog.open({ multiple: false, directory: false, filters });
  return typeof r === "string" ? r : null;
}

export async function pickFolder(title?: string): Promise<string | null> {
  if (!inTauri) return null;
  const dialog = await import("@tauri-apps/plugin-dialog");
  const r = await dialog.open({ multiple: false, directory: true, title });
  return typeof r === "string" ? r : null;
}

export async function assetUrl(path: string): Promise<string> {
  if (!inTauri) return "";
  const core = await import("@tauri-apps/api/core");
  return core.convertFileSrc(path);
}
