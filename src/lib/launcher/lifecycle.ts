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

  void indexStatus()
    .then((s) => (refs.status.current = s))
    .catch(() => {});
  void listen("hotdoc://show", () => {
    launcher.reset();
    document.getElementById("q")?.focus();
  }).then((u) => unlisteners.push(u));
  void listen("hotdoc://refresh-empty-view", () => {
    void launcher.loadEmptyView();
  }).then((u) => unlisteners.push(u));
  void listen("hotdoc://open-settings", () => {
    launcher.openSettings();
  }).then((u) => unlisteners.push(u));
  void listen("hotdoc://reload-index", () => {
    void launcher.reloadIndex();
    void indexStatus()
      .then((s) => (refs.status.current = s))
      .catch(() => {});
  }).then((u) => unlisteners.push(u));
  void listen("hotdoc://copy-diagnostics", () => {
    void launcher.copyDiagnostics();
  }).then((u) => unlisteners.push(u));
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
      unlisteners.forEach((u) => u());
      window.removeEventListener("keydown", keyHandler, true);
    },
  };
}
