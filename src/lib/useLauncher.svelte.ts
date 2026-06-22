import { writeText as clipboardWrite } from "@tauri-apps/plugin-clipboard-manager";
import {
  clearRecents,
  copyDiagnostics,
  getPopular,
  getRecents,
  hideWindow,
  listPacks,
  openUrl,
  rebuildIndex as rebuildIndexRpc,
  recordSearch,
  searchPacks,
} from "./tauri";
import type { Recent, SearchHit } from "./types";
import { log } from "./logger";
import * as recents from "./launcher/recents";
import * as pinned from "./launcher/pinned";
import { parseCommand } from "./launcher/commandMode";
import { suggestPacks } from "./launcher/suggest";

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
  popularList = $state<SearchHit[]>([]);
  pinnedIds = $state<Set<string>>(new Set());
  selectedIndex = $state<number>(-1);
  validPackIds = $state<Set<string>>(new Set());
  packFilter = $state<string | null>(null);
  settingsOpen = $state<boolean>(false);
  // ponytail: T19 — cached from boot settings + flipped by SettingsPanel.
  // Default true (recents on) so first-launch UX matches the prior behavior;
  // a real `getAllSettings()` call will overwrite this from the DB before
  // any activation can fire (App.svelte awaits it onMount).
  recentsEnabled = $state<boolean>(true);

  zeroResult = $derived(this.query.trim() !== "" && this.results.length === 0);
  emptyQuery = $derived(this.query.trim() === "");
  // ponytail: §7.6 — closest packs to the failed query; only computed
  // when there are zero results. ≤3 chips; Enter/click re-scopes.
  suggestions = $derived(this.zeroResult ? suggestPacks(this.query, this.validPackIds, 3) : []);

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

  // ponytail: T19 — called from SettingsPanel when the user toggles the
  // `recents_enabled` setting so the launcher's cache flips in lock-step
  // with the DB write (no app reload required). Reads of this field
  // happen on every `activate()`, so the cache must be authoritative.
  setRecentsEnabled(value: boolean): void {
    this.recentsEnabled = value;
  }

  // ponytail: T13 + T16. Tray "Reload index" → rebuild tantivy +
  // re-populate SQLite packs/entries tables. Returns the entry count
  // for the toast. Errors are surfaced via the existing launcher
  // toast pipeline so the user always sees a result.
  async reloadIndex(): Promise<number> {
    try {
      const n = await rebuildIndexRpc();
      this.#showToast(`Reloaded: ${n} entries`);
      void this.loadEmptyView();
      return n;
    } catch (e) {
      log.error("reload index failed", { error: String(e) });
      this.#showToast(`Reload failed: ${String(e)}`);
      throw e;
    }
  }

  async loadEmptyView() {
    try {
      const [r, p, pop] = await Promise.all([
        getRecents(5),
        pinned.fetchPinned(),
        getPopular(8).catch(() => [] as SearchHit[]),
      ]);
      this.recentList = r;
      this.pinnedList = p;
      this.pinnedIds = new Set(p.map((h) => h.id));
      // ponytail: §7.5 — Popular section is deduped against Pinned (a card
      // already shown under Pinned is not repeated).
      this.popularList = pop.filter((h) => !this.pinnedIds.has(h.id));
    } catch (e) {
      log.warn("load empty view failed", { error: String(e) });
    }
  }

  // ponytail: FR-G2 — tray/settings "Copy diagnostics". Bundle is built +
  // redacted in Rust; we just toast the result (or the error).
  async copyDiagnostics() {
    try {
      await copyDiagnostics();
      this.#showToast("Copied diagnostics to clipboard");
    } catch (e) {
      log.error("copy diagnostics failed", { error: String(e) });
      this.#showToast(`Diagnostics failed: ${String(e)}`);
    }
  }

  selectRecent(query: string) {
    this.query = query;
    void this.runSearch();
  }

  // ponytail: §7.6 — re-run the failed query scoped to a suggested pack
  // (`> <pack>` mode). Used by the zero-result suggestion chips and by
  // Enter-on-zero-result (activates the first suggestion).
  applySuggestion(packId: string) {
    this.query = `> ${packId}`;
    void this.runSearch();
  }

  async togglePin() {
    const idx = this.selectedIndex >= 0 ? this.selectedIndex : 0;
    const hit = this.results[idx];
    await this.togglePinHit(hit);
  }

  // ponytail: FR-C5 — hover/secondary actions operate on a specific hit
  // (the hovered card) rather than the keyboard-selected one.
  async togglePinHit(hit: SearchHit | undefined) {
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

  // ponytail: FR-C5 — "Copy example" copies examples[0].code, falling back
  // to syntax (same rule as Shift+Enter, FR-C2).
  async copyExample(hit: SearchHit) {
    await this.#copyAndToast(hit.example_code ?? hit.syntax);
  }

  // ponytail: FR-C5 — "Copy all" = syntax + description + first example,
  // newline-joined, skipping absent parts.
  async copyAll(hit: SearchHit) {
    const text = [hit.syntax, hit.description, hit.example_code]
      .filter((s): s is string => !!s && s.length > 0)
      .join("\n");
    await this.#copyAndToast(text);
  }

  // ponytail: FR-C5 / SEC-3 — "Open source" routes through the https-gated
  // Rust open_url IPC (same path as Ctrl+Enter). No-op without a URL.
  async openSource(hit: SearchHit) {
    if (!hit.source_url) return;
    if (!isHttpsUrl(hit.source_url)) {
      this.#showToast(`Open rejected: only https: URLs allowed`);
      return;
    }
    try {
      await openUrl(hit.source_url);
      await this.doHide();
    } catch (e) {
      log.error("open_url failed", { url: hit.source_url, error: String(e) });
      this.#showToast(`Open failed: ${String(e)}`);
    }
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
          // ponytail: §4.2.1 / §7.6 — run a real backend search for the
          // pack id, then keep only that pack's cards. This makes a bare
          // `> docker` (and the zero-result suggestion chips that re-scope
          // to `> <pack>`) actually surface the pack's entries, instead of
          // post-filtering a possibly-empty result set. Top-8 of the pack
          // is fine for the v1 dev set; revisit per-pack refine in v1.1.
          try {
            const hits = await searchPacks(cmd.pack_id);
            this.results = hits.filter((h) => h.pack_id === cmd.pack_id);
            this.selectedIndex = this.results.length > 0 ? 0 : -1;
          } catch (e) {
            this.results = [];
            this.selectedIndex = -1;
            log.error("pack filter search failed", { pack: cmd.pack_id, error: String(e) });
          }
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
      // source_url. The frontend `openUrl` (from ./tauri) calls the
      // Rust `open_url` IPC which re-checks the scheme with
      // is_https_url and refuses anything non-https. The fast-path
      // below avoids the round-trip for the obvious-reject case
      // (javascript:, file:, etc.) but the Rust gate is authoritative.
      if (isHttpsUrl(top.source_url)) {
        try {
          await openUrl(top.source_url);
          await this.doHide();
        } catch (e) {
          log.error("open_url failed", { url: top.source_url, error: String(e) });
          this.#showToast(`Open failed: ${String(e)}`);
        }
      } else {
        this.#showToast(`Open rejected: only https: URLs allowed`);
      }
      return;
    }
    const text = shift ? (top.example_code ?? top.syntax) : top.syntax;
    await this.#copyAndToast(text);
    this.#hideTimer = setTimeout(() => void this.doHide(), HIDE_AFTER_COPY_MS);
    void recents.onActivation(this.query, this.recentsEnabled);
    // ponytail: §7.5 / §9.2 — log the activation. first = top-ranked hit,
    // clicked = the row the user actually activated. Gated on the same
    // recents toggle (FR-R4); backend re-checks too.
    if (this.recentsEnabled) {
      const first = this.results[0];
      void recordSearch(this.query.trim(), first ? first.id : null, top.id).catch((e) =>
        log.warn("record search failed", { error: String(e) }),
      );
    }
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
    // ponytail: §7.6 — Enter on the zero-result state activates the first
    // suggestion (re-scopes to `> <pack>`); inert if there are none. Does
    // not close the launcher.
    if (e.key === "Enter" && this.zeroResult && this.suggestions[0]) {
      e.preventDefault();
      this.applySuggestion(this.suggestions[0]);
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
