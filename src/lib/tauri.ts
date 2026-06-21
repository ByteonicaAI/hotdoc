import { invoke } from "@tauri-apps/api/core";
import type { SearchHit } from "./types";

// ponytail: thin typed wrappers around Tauri commands. Group B will return
// Result<T, string> from the Rust side; this layer is where errors get a
// human-readable shape for the UI to toast.

export async function searchPacks(query: string): Promise<SearchHit[]> {
  return invoke<SearchHit[]>("search", { query });
}

export async function copySyntaxFor(query: string): Promise<string | null> {
  return invoke<string | null>("copy_syntax", { query });
}

export async function hideWindow(): Promise<void> {
  await invoke("hide_window");
}
