import { recordRecent } from "../tauri";
import { log } from "../logger";

// ponytail: stateless helper. Fires-and-forgets the IPC; errors are logged,
// never thrown — a write failure must not break the user's activation flow.
// Spec P1-4: recents are written on activation, not per keystroke.
export async function onActivation(query: string): Promise<void> {
  const trimmed = query.trim();
  if (!trimmed) return;
  try {
    await recordRecent(trimmed);
  } catch (e) {
    log.warn("record recent failed", { query: trimmed, error: String(e) });
  }
}
