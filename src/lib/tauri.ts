import { invoke } from "@tauri-apps/api/core";
import type { Recent, SearchHit } from "./types";

// ponytail: thin typed wrappers around Tauri commands. The Rust side
// returns Result<T, String> for search/copy_syntax; Tauri auto-rejects
// the JS promise on Err. The hook in useLauncher.svelte.ts awaits these
// directly and will let an unhandled rejection bubble — which is the
// P1-3 fix in action: the user sees a real error instead of "no
// matches forever."

export async function searchPacks(query: string): Promise<SearchHit[]> {
  return invoke<SearchHit[]>("search", { query });
}

export async function copySyntaxFor(query: string): Promise<string | null> {
  return invoke<string | null>("copy_syntax", { query });
}

export async function hideWindow(): Promise<void> {
  await invoke("hide_window");
}

export type LogLevel = "info" | "warn" | "error";

export async function logToBackend(
  level: LogLevel,
  msg: string,
  context?: Record<string, unknown>,
): Promise<void> {
  await invoke("log_error", {
    level,
    msg,
    context: context ? JSON.stringify(context) : null,
  });
}

export type { Recent };

export async function recordRecent(query: string): Promise<void> {
  await invoke("record_recent", { query });
}

export async function getRecents(n: number): Promise<Recent[]> {
  return invoke<Recent[]>("get_recents", { n });
}

export async function clearRecents(): Promise<void> {
  await invoke("clear_recents");
}

export async function pinEntry(entryId: string): Promise<void> {
  await invoke("pin_entry", { entryId });
}

export async function unpinEntry(entryId: string): Promise<void> {
  await invoke("unpin_entry", { entryId });
}

export async function getPinned(): Promise<SearchHit[]> {
  return invoke<SearchHit[]>("get_pinned");
}

export async function listPacks(): Promise<string[]> {
  return invoke<string[]>("list_packs");
}

export type SettingsMap = Record<string, string>;

export async function setSetting(key: string, value: string): Promise<void> {
  await invoke("set_setting", { key, value });
}

export async function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>("get_setting", { key });
}

export async function getAllSettings(): Promise<SettingsMap> {
  return invoke<SettingsMap>("get_all_settings");
}

export async function setHotkey(combo: string): Promise<void> {
  await invoke("set_hotkey", { combo });
}

export async function setAutostart(enabled: boolean): Promise<void> {
  await invoke("set_autostart", { enabled });
}

export async function rebuildIndex(): Promise<number> {
  // ponytail: T13. Returns the entry count of the freshly-rebuilt
  // index. The frontend toasts the count and refreshes the empty
  // view so the pinned list picks up any new entries.
  return invoke<number>("rebuild_index");
}

// ponytail: SEC-3 / FR-C3. Wraps the Rust `open_url` IPC command.
// The Rust side re-validates the scheme (https: only) before calling
// the opener plugin; this thin wrapper exists so the frontend stays
// consistent with the other IPC calls and never imports
// `@tauri-apps/plugin-opener` for URL opens.
export async function openUrl(url: string): Promise<void> {
  await invoke("open_url", { url });
}

// ponytail: §7.5 / §9.2 — record one activation in the search log. ids may
// be null for a zero-result activation. Backend honors the recents-enabled
// toggle (FR-R4).
export async function recordSearch(
  query: string,
  firstId: string | null,
  clickedId: string | null,
): Promise<void> {
  await invoke("record_search", { query, firstId, clickedId });
}

// ponytail: §7.5 — most-activated cards for the empty-view Popular section.
export async function getPopular(n: number): Promise<SearchHit[]> {
  return invoke<SearchHit[]>("get_popular", { n });
}

export type IndexStatus = { entry_count: number; pack_count: number };

// ponytail: FR-I4 — index counts for the launcher footer.
export async function indexStatus(): Promise<IndexStatus> {
  return invoke<IndexStatus>("index_status");
}

// ponytail: FR-G2 — copy the redacted diagnostics bundle; returns it for
// the toast.
export async function copyDiagnostics(): Promise<string> {
  return invoke<string>("copy_diagnostics");
}
