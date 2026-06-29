// ponytail: WS-H — extracted from App.svelte's onMount. Owns the 5 IPC
// event listener wirings (hotdoc://show, refresh-empty-view, open-settings,
// reload-index, copy-diagnostics), boot settings + theme apply, status +
// version fetch, and the keydown handler. Returns a single cleanup that
// detaches every listener. App.svelte calls this once on mount and
// invokes the cleanup on teardown.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getAllSettings, indexStatus } from "../tauri";
import type { Launcher } from "../useLauncher.svelte";

export type LifecycleStatus = { entry_count: number; pack_count: number };

export interface LifecycleRefs {
  status: { current: LifecycleStatus | null };
  appVersion: { current: string };
}

export interface LifecycleResult {
  cleanup: () => void;
}

export function setupLauncher(launcher: Launcher, refs: LifecycleRefs): LifecycleResult {
  const unlisteners: UnlistenFn[] = [];
  let disposed = false;
  const keyHandler = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      if (launcher.detailsHit) {
        launcher.closeDetails();
        document.getElementById("q")?.focus();
      } else {
        void launcher.doHide();
      }
    }
  };
  window.addEventListener("keydown", keyHandler, true);
  document.getElementById("q")?.focus();

  // ponytail: I6 fix — if cleanup() runs before listen()'s promise
  // resolves, the resulting unlisten fn would otherwise be pushed to an
  // array that's about to be discarded (ghost listener). track() either
  // queues it for the normal cleanup drain, or fires it immediately when
  // teardown already happened.
  function track(p: Promise<UnlistenFn>) {
    void p.then((u) => {
      if (disposed) u();
      else unlisteners.push(u);
    });
  }

  void indexStatus()
    .then((s) => (refs.status.current = s))
    .catch(() => {});
  track(
    listen("hotdoc://show", () => {
      launcher.reset();
      document.getElementById("q")?.focus();
    }),
  );
  track(
    listen("hotdoc://refresh-empty-view", () => {
      void launcher.loadEmptyView();
    }),
  );
  track(
    listen("hotdoc://open-settings", () => {
      launcher.openSettings();
    }),
  );
  track(
    listen("hotdoc://reload-index", () => {
      void launcher.reloadIndex();
      void indexStatus()
        .then((s) => (refs.status.current = s))
        .catch(() => {});
    }),
  );
  track(
    listen("hotdoc://copy-diagnostics", () => {
      void launcher.copyDiagnostics();
    }),
  );
  void getAllSettings()
    .then((s) => {
      launcher.applyTheme(s["theme"]);
      launcher.setRecentsEnabled(s["recents_enabled"] !== "false");
    })
    .catch(() => {});
  void launcher.loadEmptyView();
  void launcher.initPalette();
  void getVersion()
    .then((v) => (refs.appVersion.current = v))
    .catch(() => {});

  return {
    cleanup: () => {
      disposed = true;
      unlisteners.forEach((u) => u());
      window.removeEventListener("keydown", keyHandler, true);
    },
  };
}
