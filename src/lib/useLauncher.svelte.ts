import { writeText as clipboardWrite } from "@tauri-apps/plugin-clipboard-manager";
import {
  clearRecents,
  copyDiagnostics,
  getRecents,
  hideWindow,
  listPackMetas,
  listPacks,
  openUrl,
  rebuildIndex as rebuildIndexRpc,
  recordSearch,
  searchPacks,
} from "./tauri";
import type { PackMeta, Recent, SearchHit } from "./types";
import { log } from "./logger";
import * as recents from "./launcher/recents";
import * as pinned from "./launcher/pinned";
import { parseCommand } from "./launcher/commandMode";
import { suggestPacks } from "./launcher/suggest";
import { STRINGS, format } from "./strings";
import { isHttpsUrl } from "./url";

// ponytail: Svelte 5 hooks live in `.svelte.ts` files. The runes `$state` and
// `$derived` only track reactively when fields are read directly off `this`.
// In the template, every handler is wrapped as `(e) => launcher.onFoo(e)` so
// `this` binds to the instance — otherwise the event listener calls the
// method with the DOM element as `this` and all reads go through `undefined`.

const SEARCH_DEBOUNCE_MS = 30;
const TOAST_MS = 1000;
// Enter-to-copy dismiss choreography (spotlight-style): the search view fades
// out, the "Copied" confirmation fades in and holds, then it fades out and the
// window hides. These gaps gate the phase transitions; the matching visual fade
// durations live in App.svelte and must stay ≤ these.
const SEARCH_FADE_MS = 160;
const COPIED_HOLD_MS = 850;
const DISMISS_FADE_MS = 200;

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
  // Dismiss choreography phase. "search" = normal launcher; "copied" = the
  // search view has faded out and the "Copied" confirmation is shown; "closing"
  // = the confirmation is fading out before the window hides.
  phase = $state<"search" | "copied" | "closing">("search");
  recentList = $state<Recent[]>([]);
  pinnedList = $state<SearchHit[]>([]);
  pinnedIds = $state<Set<string>>(new Set());
  selectedIndex = $state<number>(-1);
  validPackIds = $state<Set<string>>(new Set());
  packFilter = $state<string | null>(null);
  settingsOpen = $state<boolean>(false);
  aboutOpen = $state<boolean>(false);
  detailsHit = $state<SearchHit | null>(null);
  packMetas = $state<PackMeta[]>([]);
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
  // ponytail: T11 NFR-9 — total empty-view row count for arrow-nav bounds.
  // Pinned first, then Recents — same ordering as EmptyView renders.
  emptyItemCount = $derived(this.pinnedList.length + this.recentList.length);

  #toastTimer: ReturnType<typeof setTimeout> | null = null;
  #hideTimer: ReturnType<typeof setTimeout> | null = null;
  #debounce: ReturnType<typeof setTimeout> | null = null;

  #showToast(msg: string) {
    this.toast = msg;
    if (this.#toastTimer) clearTimeout(this.#toastTimer);
    this.#toastTimer = setTimeout(() => (this.toast = null), TOAST_MS);
  }

  // ponytail: M4.5-T4 — public toast entry point for child
  // components (AboutPanel, DetailsPane) that need to surface a
  // rejection/error without owning the toast pipeline. Wraps the
  // private #showToast so the timer logic stays in one place.
  showToast(msg: string): void {
    this.#showToast(msg);
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
      this.#showToast(format(STRINGS.TOAST_RELOAD_OK, String(n)));
      void this.loadEmptyView();
      return n;
    } catch (e) {
      log.error("reload index failed", { error: String(e) });
      this.#showToast(format(STRINGS.TOAST_RELOAD_FAIL, String(e)));
      throw e;
    }
  }

  async loadEmptyView() {
    // ponytail: M4.5-T1 / FR-R4 — when recents are disabled, skip the
    // recents fetch and clear recentList so EmptyView doesn't briefly
    // render stale rows from before the toggle.
    if (!this.recentsEnabled) this.recentList = [];
    try {
      const [r, p] = await Promise.all([
        this.recentsEnabled ? getRecents(5) : Promise.resolve([] as Recent[]),
        pinned.fetchPinned(),
      ]);
      this.recentList = r ?? [];
      this.pinnedList = p;
      this.pinnedIds = new Set(p.map((h) => h.id));
    } catch (e) {
      log.warn("load empty view failed", { error: String(e) });
    }
  }

  // ponytail: FR-G2 — tray/settings "Copy diagnostics". Bundle is built +
  // redacted in Rust; we just toast the result (or the error).
  async copyDiagnostics() {
    try {
      await copyDiagnostics();
      this.#showToast(STRINGS.TOAST_DIAG_OK);
    } catch (e) {
      log.error("copy diagnostics failed", { error: String(e) });
      this.#showToast(format(STRINGS.TOAST_DIAG_FAIL, String(e)));
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
      this.#showToast(format(STRINGS.TOAST_PIN, hit.title));
    } else {
      updated.delete(hit.id);
      this.#showToast(format(STRINGS.TOAST_UNPIN, hit.title));
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
      this.#showToast(STRINGS.TOAST_OPEN_REJECTED);
      return;
    }
    try {
      await openUrl(hit.source_url);
      await this.doHide();
    } catch (e) {
      log.error("open_url failed", { url: hit.source_url, error: String(e) });
      this.#showToast(format(STRINGS.TOAST_OPEN_FAIL, String(e)));
    }
  }

  selectPinned(hit: SearchHit) {
    this.query = hit.syntax;
    void this.runSearch();
  }

  // ponytail: T11 — Enter on empty-view copies the selected item's syntax and
  // hides (same UX as main search Enter). Pinned always has .syntax; recents
  // use copied_syntax if recorded, else fall back to populating the search box.
  async activateEmpty() {
    const i = this.selectedIndex;
    if (i < 0) return;
    const pLen = this.pinnedList.length;
    const rLen = this.recentList.length;
    if (i < pLen) {
      const p = this.pinnedList[i];
      if (p) {
        await this.#copyThenDismiss(p.syntax);
      }
    } else if (i < pLen + rLen) {
      const r = this.recentList[i - pLen];
      if (r) {
        if (r.copied_syntax) {
          await this.#copyThenDismiss(r.copied_syntax);
        } else {
          this.selectRecent(r.query);
        }
      }
    }
  }

  async copyText(text: string) {
    await this.#copyThenDismiss(text);
  }

  // ponytail: T11 — Ctrl+P on empty-view pins/unpins the selected pinned row.
  async togglePinEmpty() {
    const i = this.selectedIndex;
    if (i < 0) return;
    const pLen = this.pinnedList.length;
    if (i >= pLen) return; // recents: no pin action
    const hit = this.pinnedList[i];
    if (hit) await this.togglePinHit(hit);
  }

  openSettings() {
    this.settingsOpen = true;
  }

  closeSettings() {
    this.settingsOpen = false;
  }

  openAbout() {
    this.aboutOpen = true;
  }

  closeAbout() {
    this.aboutOpen = false;
  }

  openDetails(hit: SearchHit) {
    this.detailsHit = hit;
  }

  closeDetails() {
    this.detailsHit = null;
  }

  // ponytail: WS-F — two known fixed window heights; window snaps between
  // them and the inner content (`.search-view`) uses a CSS transition on
  // `height` to mask the snap. Settings/About force tall; the details
  // popout is rendered inline so compact (660) still fits it.
  private static readonly HEIGHTS = { compact: 660, tall: 820 } as const;

  private get desiredWindowHeight(): number {
    if (this.settingsOpen || this.aboutOpen) return Launcher.HEIGHTS.tall;
    // open the details pane inline; compact still fits 5 entries
    return Launcher.HEIGHTS.compact;
  }

  /** Snap to the launcher height implied by current phase. No animation
   *  on the window itself — `main`'s CSS owns the visual continuity. */
  applyWindowHeight(): void {
    if (typeof window === "undefined") return;
    const target = this.desiredWindowHeight;
    const el = document.documentElement;
    el.style.setProperty("--launcher-height", `${target}px`);
  }

  // ponytail: caller must invoke this from a component scope (App.svelte
  // <script>) — `$effect` requires an active component context, so a
  // bare `new Launcher()` outside one (e.g. App.test.ts's applyTheme
  // suite) would throw `effect_orphan`. Keeping the effect one method
  // hop away from the constructor avoids that trap without losing the
  // binding convenience.
  /** Bind `$effect` to caller's scope; App.svelte invokes this once. */
  bindWindowHeightEffect(): void {
    $effect(() => {
      // touch each phase field so the effect re-runs on changes
      void this.settingsOpen;
      void this.aboutOpen;
      this.applyWindowHeight();
    });
  }

  openHelp() {
    this.#showToast(STRINGS.TOAST_HELP);
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
      const [ids, metas] = await Promise.all([listPacks(), listPackMetas()]);
      this.validPackIds = new Set(ids);
      this.packMetas = metas;
    } catch (e) {
      log.warn("list packs failed", { error: String(e) });
    }
  }

  async doHide() {
    this.#clearTimers();
    try {
      await hideWindow();
    } catch (e) {
      // The window staying visible is recoverable (next hotdoc://show re-syncs),
      // but log it so a stuck window isn't completely invisible to diagnostics.
      log.error("hide window failed", { error: String(e) });
    }
    // reset() restores a clean state regardless of whether hideWindow threw, so
    // the next show always starts fresh even if hotdoc://show doesn't fire.
    this.reset();
  }

  // Authoritative clean state. Called on hide AND as the hotdoc://show re-show
  // entry point, so it must clear the dismiss choreography (phase/timers/toast)
  // too — otherwise re-summoning mid-dismiss leaves the search view unmounted
  // with a stale "Copied" label and an in-flight timer that hides the window.
  reset() {
    this.#clearTimers();
    this.phase = "search";
    this.toast = null;
    this.query = "";
    this.results = [];
    this.selectedIndex = -1;
    void this.loadEmptyView();
  }

  async runSearch() {
    const q = this.query.trim();
    if (this.detailsHit) this.closeDetails();
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
        case "about":
          this.openAbout();
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
      this.#showToast(format(STRINGS.TOAST_SEARCH_FAIL, String(e)));
    }
  }

  onInput(value: string) {
    this.query = value;
    if (this.#debounce) clearTimeout(this.#debounce);
    this.#debounce = setTimeout(() => void this.runSearch(), SEARCH_DEBOUNCE_MS);
  }

  // Returns whether the clipboard write succeeded so callers can branch (the
  // dismiss choreography and recents recording must not run on a failed copy).
  async #copyAndToast(text: string): Promise<boolean> {
    try {
      await clipboardWrite(text);
      this.#showToast(format(STRINGS.TOAST_COPY_OK, text));
      return true;
    } catch (e) {
      log.error("copy failed", { text, error: String(e) });
      this.#showToast(format(STRINGS.TOAST_COPY_FAIL, String(e)));
      return false;
    }
  }

  // Enter-to-copy path: copy, then run the spotlight-style dismiss — fade the
  // search view out, show the "Copied" confirmation, hold, fade it out, hide.
  // The phase transitions are timed here; App.svelte renders each phase with a
  // matching fade. Button copies (Copy example/all) use #copyAndToast instead
  // and leave the launcher open. Returns false (and skips the dismiss) when the
  // copy failed, leaving the launcher open in "search" with the failure toast.
  async #copyThenDismiss(text: string): Promise<boolean> {
    if (!(await this.#copyAndToast(text))) return false;
    // We drive the confirmation's lifetime through the phases below, so cancel
    // #showToast's own auto-clear (otherwise it would blank the label mid-hold).
    this.#clearTimers();
    this.phase = "copied";
    // Reused for two sequential one-shots: the inner timer is only assigned from
    // inside the outer callback (after it has fired), so it never clobbers a
    // live timer. Safe only because the search view — and thus the activation
    // key path — is unmounted while phase !== "search".
    this.#hideTimer = setTimeout(() => {
      this.phase = "closing";
      this.#hideTimer = setTimeout(() => void this.doHide(), DISMISS_FADE_MS);
    }, SEARCH_FADE_MS + COPIED_HOLD_MS);
    return true;
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
          this.#showToast(format(STRINGS.TOAST_OPEN_FAIL, String(e)));
        }
      } else {
        this.#showToast(STRINGS.TOAST_OPEN_REJECTED);
      }
      return;
    }
    const text = shift ? (top.example_code ?? top.syntax) : top.syntax;
    // Only record the activation if the copy actually landed on the clipboard —
    // otherwise a failed copy would persist a bogus one-keystroke-copy recent.
    if (!(await this.#copyThenDismiss(text))) return;
    void recents.onActivation(this.query, top.syntax, this.recentsEnabled);
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
      if (this.detailsHit) {
        e.preventDefault();
        this.closeDetails();
        // ponytail: FR-C6 — after the details pane closes, focus
        // returns to the search input so the user can keep typing.
        // App.svelte's onMount focuses on first paint; this restores
        // that focus after the Tab detour.
        document.getElementById("q")?.focus();
        return;
      }
      e.preventDefault();
      void this.doHide();
      return;
    }
    // ponytail: FR-C6 — Tab toggles the details pane for the selected result.
    if (e.key === "Tab" && this.results.length > 0 && this.selectedIndex >= 0) {
      e.preventDefault();
      if (this.detailsHit) {
        this.closeDetails();
      } else {
        const hit = this.results[this.selectedIndex];
        if (hit) this.openDetails(hit);
      }
      return;
    }
    // ponytail: T11 NFR-9 — empty-state (no query) arrow nav over Recents/Pinned/Popular.
    if (this.emptyQuery && this.emptyItemCount > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        this.selectedIndex =
          this.selectedIndex < 0 ? 0 : (this.selectedIndex + 1) % this.emptyItemCount;
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        this.selectedIndex =
          this.selectedIndex <= 0 ? this.emptyItemCount - 1 : this.selectedIndex - 1;
        return;
      }
      if (e.key === "Enter" && this.selectedIndex >= 0) {
        e.preventDefault();
        void this.activateEmpty();
        return;
      }
      if ((e.key === "p" || e.key === "P") && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        void this.togglePinEmpty();
        return;
      }
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
