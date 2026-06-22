import { writeText as clipboardWrite } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { clearRecents, getRecents, hideWindow, listPacks, searchPacks } from "./tauri";
import type { Recent, SearchHit } from "./types";
import { log } from "./logger";
import * as recents from "./launcher/recents";
import * as pinned from "./launcher/pinned";
import { parseCommand } from "./launcher/commandMode";

function isHttpsUrl(u: string): boolean {
  try {
    return new URL(u).protocol === "https:";
  } catch {
    return false;
  }
}

// ponytail: Svelte 5 hooks live in `.svelte.ts` files. The runes `$state` and
// `$derived` only track reactively when fields are read directly off `this`.
// In the template, every handler is wrapped as `(e) => launcher.onFoo(e)` so
// `this` binds to the instance — otherwise the event listener calls the
// method with the DOM element as `this` and all reads go through `undefined`.

const SEARCH_DEBOUNCE_MS = 30;
const TOAST_MS = 1000;
const HIDE_AFTER_COPY_MS = 300;

// ponytail: closed enum per PRD §6.8 — three values, no "blue". The DB
// stores the value as a string and a stale row (e.g. from a removed
// setting) can land here. applyTheme rejects anything outside the set
// and falls back to "system" so the launcher's CSS @media query still
// follows the OS.
export type Theme = "light" | "dark" | "system";
const VALID_THEMES: ReadonlySet<Theme> = new Set<Theme>(["light", "dark", "system"]);

export class Launcher {
  query = $state("");
  results = $state<SearchHit[]>([]);
  toast = $state<string | null>(null);
  recentList = $state<Recent[]>([]);
  pinnedList = $state<SearchHit[]>([]);
  pinnedIds = $state<Set<string>>(new Set());
  selectedIndex = $state<number>(-1);
  validPackIds = $state<Set<string>>(new Set());
  packFilter = $state<string | null>(null);
  settingsOpen = $state<boolean>(false);

  zeroResult = $derived(this.query.trim() !== "" && this.results.length === 0);
  emptyQuery = $derived(this.query.trim() === "");

  #toastTimer: ReturnType<typeof setTimeout> | null = null;
  #hideTimer: ReturnType<typeof setTimeout> | null = null;
  #debounce: ReturnType<typeof setTimeout> | null = null;

  #showToast(msg: string) {
    this.toast = msg;
    if (this.#toastTimer) clearTimeout(this.#toastTimer);
    this.#toastTimer = setTimeout(() => (this.toast = null), TOAST_MS);
  }

  #clearTimers() {
    if (this.#toastTimer) {
      clearTimeout(this.#toastTimer);
      this.#toastTimer = null;
    }
    if (this.#hideTimer) {
      clearTimeout(this.#hideTimer);
      this.#hideTimer = null;
    }
  }

  reset() {
    this.#clearTimers();
    this.query = "";
    this.results = [];
    this.toast = null;
  }

  async loadEmptyView() {
    try {
      const [r, p] = await Promise.all([getRecents(5), pinned.fetchPinned()]);
      this.recentList = r;
      this.pinnedList = p;
      this.pinnedIds = new Set(p.map((h) => h.id));
    } catch (e) {
      log.warn("load empty view failed", { error: String(e) });
    }
  }

  selectRecent(query: string) {
    this.query = query;
    void this.runSearch();
  }

  async togglePin() {
    const idx = this.selectedIndex >= 0 ? this.selectedIndex : 0;
    const hit = this.results[idx];
    if (!hit) return;
    const next = await pinned.toggle(hit.id, this.pinnedIds.has(hit.id));
    const updated = new Set(this.pinnedIds);
    if (next) {
      updated.add(hit.id);
      this.#showToast(`Pinned: ${hit.title}`);
    } else {
      updated.delete(hit.id);
      this.#showToast(`Unpinned: ${hit.title}`);
    }
    this.pinnedIds = updated;
    void this.loadEmptyView();
  }

  selectPinned(hit: SearchHit) {
    this.query = hit.syntax;
    void this.runSearch();
  }

  openSettings() {
    this.settingsOpen = true;
  }

  closeSettings() {
    this.settingsOpen = false;
  }

  openHelp() {
    this.#showToast(
      "↑/↓ navigate · Enter copy · Shift+Enter example · Ctrl+P pin · Ctrl+Shift+? palette",
    );
  }

  // ponytail: single attribute flip on <html>. "system" removes the
  // attribute so app.css @media (prefers-color-scheme: …) picks the
  // colors. Unknown values (stale DB rows, manual edits) fall back to
  // "system" — better to follow the OS than to render unstyled.
  applyTheme(value: unknown): Theme {
    const v: Theme = VALID_THEMES.has(value as Theme) ? (value as Theme) : "system";
    if (v !== value) {
      log.warn("applyTheme rejected unknown value", { value: String(value) });
    }
    if (typeof document !== "undefined") {
      if (v === "system") {
        delete document.documentElement.dataset.theme;
      } else {
        document.documentElement.dataset.theme = v;
      }
    }
    return v;
  }

  async initPalette(): Promise<void> {
    // ponytail: T6 — populates validPackIds for `> <pack_id>` filter
    // validation. Fire-and-forget from App.svelte onMount. Race: the
    // first palette parse may run before this resolves; parseCommand
    // treats an empty Set as "no valid pack ids" and falls through to
    // search. Acceptable — the user sees `> dctr` search instead of
    // pack-filter for ~10ms at app start.
    try {
      const ids = await listPacks();
      this.validPackIds = new Set(ids);
    } catch (e) {
      log.warn("list packs failed", { error: String(e) });
    }
  }

  async doHide() {
    this.#clearTimers();
    await hideWindow();
  }

  async runSearch() {
    const q = this.query.trim();
    if (!q) {
      this.results = [];
      this.selectedIndex = -1;
      this.packFilter = null;
      return;
    }
    if (q.startsWith(">")) {
      const cmd = parseCommand(q, this.validPackIds);
      switch (cmd.kind) {
        case "recents":
          this.query = "";
          await this.loadEmptyView();
          return;
        case "settings":
          this.openSettings();
          return;
        case "help":
          this.openHelp();
          return;
        case "recents-clear":
          this.query = "";
          try {
            await clearRecents();
          } catch (e) {
            log.warn("clear recents failed", { error: String(e) });
          }
          await this.loadEmptyView();
          return;
        case "pack-filter":
          this.packFilter = cmd.pack_id;
          // ponytail: post-filter the existing results rather than adding
          // pack_id to the Rust search signature — top-8 results filtered
          // client-side is fine for the 5-pack dev set; revisit if v1.1
          // telemetry shows users refining within a pack.
          this.results = this.results.filter((h) => h.pack_id === cmd.pack_id);
          this.selectedIndex = this.results.length > 0 ? 0 : -1;
          return;
        case "fallthrough":
          this.query = cmd.query;
          this.packFilter = null;
          // Re-enter the search path with the new query. Falling through
          // to the bottom of this function would re-run with the stale
          // outer `q` (= "> dctr"), not the stripped "dctr".
          await this.runSearch();
          return;
      }
    } else {
      this.packFilter = null;
    }
    try {
      this.results = await searchPacks(q);
      this.selectedIndex = this.results.length > 0 ? 0 : -1;
    } catch (e) {
      this.results = [];
      this.selectedIndex = -1;
      log.error("search failed", { query: q, error: String(e) });
      this.#showToast(`Search failed: ${String(e)}`);
    }
  }

  onInput(value: string) {
    this.query = value;
    if (this.#debounce) clearTimeout(this.#debounce);
    this.#debounce = setTimeout(() => void this.runSearch(), SEARCH_DEBOUNCE_MS);
  }

  async #copyAndToast(text: string) {
    try {
      await clipboardWrite(text);
      this.#showToast(`Copied: ${text}`);
    } catch (e) {
      log.error("copy failed", { text, error: String(e) });
      this.#showToast(`Copy failed: ${String(e)}`);
    }
  }

  async activate(shift: boolean, ctrl: boolean) {
    const idx = this.selectedIndex >= 0 ? this.selectedIndex : 0;
    const top = this.results[idx];
    if (!top) return;
    if (ctrl && top.source_url) {
      // ponytail: SEC-3 / FR-C3 — pack content can carry any string in
      // source_url. openUrl() under the granted `opener:allow-open-url`
      // capability would open javascript:, file:, or any scheme the OS
      // supports. Allow https: only; reject and toast everything else.
      if (isHttpsUrl(top.source_url)) {
        await openUrl(top.source_url);
        await this.doHide();
      } else {
        this.#showToast(`Open rejected: only https: URLs allowed`);
      }
      return;
    }
    const text = shift ? (top.example_code ?? top.syntax) : top.syntax;
    await this.#copyAndToast(text);
    this.#hideTimer = setTimeout(() => void this.doHide(), HIDE_AFTER_COPY_MS);
    void recents.onActivation(this.query);
  }

  onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      void this.doHide();
      return;
    }
    if (e.key === "ArrowDown" && this.results.length > 0) {
      e.preventDefault();
      this.selectedIndex = (this.selectedIndex + 1) % this.results.length;
      return;
    }
    if (e.key === "ArrowUp" && this.results.length > 0) {
      e.preventDefault();
      this.selectedIndex =
        this.selectedIndex <= 0 ? this.results.length - 1 : this.selectedIndex - 1;
      return;
    }
    if (e.key === "Enter" && this.results.length > 0) {
      e.preventDefault();
      void this.activate(e.shiftKey, e.ctrlKey || e.metaKey);
      return;
    }
    if ((e.key === "p" || e.key === "P") && (e.ctrlKey || e.metaKey) && this.results.length > 0) {
      e.preventDefault();
      void this.togglePin();
      return;
    }
    if (e.key === "c" && (e.ctrlKey || e.metaKey) && this.query === "") {
      e.preventDefault();
      void this.doHide();
    }
  }
}
