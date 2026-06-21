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
