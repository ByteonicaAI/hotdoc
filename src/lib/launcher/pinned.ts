import { getPinned, pinEntry, unpinEntry } from "../tauri";
import { log } from "../logger";

// ponytail: stateless helper for pin toggling. Errors are logged, never
// thrown — Ctrl+P is a one-keystroke action and a write failure must not
// break the user's flow. Spec FR-C4 + FR-P1–P3.
export async function toggle(entryId: string, currentlyPinned: boolean): Promise<boolean> {
  try {
    if (currentlyPinned) {
      await unpinEntry(entryId);
      return false;
    }
    await pinEntry(entryId);
    return true;
  } catch (e) {
    log.warn("pin toggle failed", { entryId, error: String(e) });
    return currentlyPinned;
  }
}

export async function fetchPinned(): Promise<Awaited<ReturnType<typeof getPinned>>> {
  try {
    return await getPinned();
  } catch (e) {
    log.warn("fetch pinned failed", { error: String(e) });
    return [];
  }
}
