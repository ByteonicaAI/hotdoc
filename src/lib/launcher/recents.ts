import { recordRecent } from "../tauri";
import { log } from "../logger";

// ponytail: stateless helper. Fires-and-forgets the IPC; errors are logged,
// never thrown — a write failure must not break the user's activation flow.
// Spec P1-4: recents are written on activation, not per keystroke.
// T19 (FR-R4): `recentsEnabled` short-circuits the IPC when the user has
// disabled recents in Settings — backend `recents::record` also gates
// against `is_enabled`, so this is purely an optimization + a way to
// avoid noisy log entries when the toggle is off.
export async function onActivation(
  query: string,
  syntax: string,
  recentsEnabled = true,
): Promise<void> {
  const trimmed = query.trim();
  if (!trimmed || !recentsEnabled) return;
  try {
    await recordRecent(trimmed, syntax || null);
  } catch (e) {
    log.warn("record recent failed", { query: trimmed, error: String(e) });
  }
}
