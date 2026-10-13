import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ItemEvent, ItemView, LogEntry, Params, StepLog, VersionInfo } from "./types";

export const api = {
  listItems: () => invoke<ItemView[]>("list_items"),
  listVersions: (id: string) => invoke<VersionInfo[]>("list_versions", { id }),
  startItem: (id: string, version: string | null, params: Params) =>
    invoke<void>("start_item", { id, version, params }),
  runConfig: (id: string, mirrorUrl: string | null, params: Params) =>
    invoke<StepLog[]>("run_config", { id, mirrorUrl, params }),
  isElevated: () => invoke<boolean>("is_elevated"),
  configDefaults: (id: string) => invoke<Record<string, string>>("config_defaults", { id }),
  dirNeedsConfirm: (dir: string) => invoke<boolean>("dir_needs_confirm", { dir }),
  readPublicKey: (file?: string) => invoke<string>("read_public_key", { file: file ?? null }),
  listSshPublicKeys: () => invoke<string[]>("list_ssh_public_keys"),
  sshKeyStatus: () => invoke<{ has_public: boolean; has_private: boolean }>("ssh_key_status"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  listEnvBackups: () => invoke<string[]>("list_env_backups"),
  readEnvBackup: (file: string) => invoke<string>("read_env_backup", { file }),
  deleteEnvBackup: (file: string) => invoke<void>("delete_env_backup", { file }),
  listCacheEntries: () => invoke<{ name: string; size: number; modified: string }[]>("list_cache_entries"),
  deleteCacheEntry: (name: string) => invoke<void>("delete_cache_entry", { name }),
  clearCache: () => invoke<number>("clear_cache"),
  openCacheDir: () => invoke<void>("open_cache_dir"),
  appVersion: () => invoke<string>("app_version"),
  restoreEnvBackup: (file: string) => invoke<void>("restore_env_backup", { file }),
  cacheSizeMb: () => invoke<number>("cache_size_mb"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  testSsh: () => invoke<string>("test_ssh"),
  setLanguage: (lang: string) => invoke<void>("set_language", { lang }),
};

export function onItemState(cb: (e: ItemEvent) => void) {
  return listen<ItemEvent>("item-state", (ev) => cb(ev.payload));
}

export function onLog(cb: (e: LogEntry) => void) {
  return listen<LogEntry>("log", (ev) => cb(ev.payload));
}
