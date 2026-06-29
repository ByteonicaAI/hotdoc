<script lang="ts">
  import { onMount } from "svelte";
  import { prefersReducedMotion } from "svelte/motion";
  import { listen } from "@tauri-apps/api/event";
  import { getVersion } from "@tauri-apps/api/app";
  import SearchView from "./lib/SearchView.svelte";
  import CopiedView from "./lib/CopiedView.svelte";
  import Toast from "./lib/Toast.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import AboutPanel from "./lib/AboutPanel.svelte";
  import { Launcher } from "./lib/useLauncher.svelte";
  import { getAllSettings, indexStatus, openUrl } from "./lib/tauri";
  import type { SearchHit } from "./lib/types";
  import { STRINGS } from "./lib/strings";

  const launcher = new Launcher();

  // Visual fade durations for the Enter-to-copy dismiss choreography. Must stay
  // ≤ the phase gaps in useLauncher.svelte.ts (SEARCH_FADE_MS / DISMISS_FADE_MS)
  // so each fade finishes before the phase advances.
  const SEARCH_FADE_MS = 160;
  const COPIED_FADE_MS = 200;
  function fadeMs(ms: number): number {
    return prefersReducedMotion.current ? 0 : ms;
  }

  // Show the results area (and footer) once there's a query, or when there are
  // recents/pinned to surface. Empty + no history = just the search bar; the
  // rest of the (transparent) window stays empty until the user types.
  const showResults = $derived(
    !launcher.emptyQuery || launcher.pinnedList.length + launcher.recentList.length > 0,
  );

  // The window is a fixed-size transparent pane; only the rounded `main` shows.
  // Clicking the empty area (the backdrop) dismisses, like clicking outside
  // Spotlight.
  function dismiss() {
    void launcher.doHide();
  }

  $effect(() => {
    const _ = launcher.selectedIndex;
    document.querySelector('[role="listbox"] li.active')?.scrollIntoView({ block: "nearest" });
  });
  // ponytail: FR-I4 — footer index state. Cold indexing is synchronous in
  // Rust before this window paints, so a streaming N/N counter would never
  // render; the steady-state count is the honest signal. `null` = not yet
  // loaded → "Indexing…".
  let status = $state<{ entry_count: number; pack_count: number } | null>(null);
  let appVersion = $state("0.4.0");

  // Footer key hints, split into discrete chips. Contextual to whether the
  // user is searching (full set) or on the cold/empty view (minimal set).
  const footerKeys = $derived(
    (launcher.emptyQuery || launcher.results.length === 0
      ? STRINGS.FOOTER_KEYS_EMPTY
      : STRINGS.FOOTER_KEYS_SEARCH
    )
      .split("·")
      .map((s) => s.trim())
      .filter(Boolean),
  );

  function reportUrl(hit: SearchHit): string {
    const title = encodeURIComponent(`Card report: ${hit.pack_id}/${hit.id}`);
    const body = encodeURIComponent(
      `**Card ID:** ${hit.pack_id}/${hit.id}\n**Syntax:** ${hit.syntax}`,
    );
    return `https://github.com/ByteonicaAI/hotdoc/issues/new?template=card-report.md&title=${title}&body=${body}`;
  }

  async function reportCard(hit: SearchHit) {
    await openUrl(reportUrl(hit));
  }

  onMount(() => {
    document.getElementById("q")?.focus();
    const unlisteners: Array<() => void> = [];
    function onWindowKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        if (launcher.detailsHit) {
          launcher.closeDetails();
          document.getElementById("q")?.focus();
        } else {
          void launcher.doHide();
        }
      }
    }
    window.addEventListener("keydown", onWindowKey, true);
    void indexStatus()
      .then((s) => (status = s))
      .catch(() => {
        /* leave as Indexing… */
      });
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
      // ponytail: T13 + T16. Tray menu item → IPC rebuild →
      // reloadIndex() in the launcher toasts the count and refreshes
      // the empty view. Errors propagate to the toast pipeline.
      void launcher.reloadIndex();
      void indexStatus()
        .then((s) => (status = s))
        .catch(() => {});
    }).then((u) => unlisteners.push(u));
    void listen("hotdoc://copy-diagnostics", () => {
      // ponytail: FR-G2 — tray "Copy diagnostics" → redacted bundle to
      // clipboard, toast confirms.
      void launcher.copyDiagnostics();
    }).then((u) => unlisteners.push(u));
    // ponytail: read the persisted theme synchronously after focus but
    // before the first paint of user-driven content. applyTheme is a
    // pure DOM flip — no flash because the cascade resolves on the same
    // microtask as the attribute write.
    void getAllSettings()
      .then((s) => {
        launcher.applyTheme(s["theme"]);
        // ponytail: T19 (FR-R4) — sync the recents toggle into the
        // launcher's cache before any activation can fire. The cache
        // is also kept in sync by SettingsPanel.setRecentsEnabled on
        // user toggle; this is the boot path.
        launcher.setRecentsEnabled(s["recents_enabled"] !== "false");
      })
      .catch(() => {
        /* default theme (system) and recents (on) are fine */
      });
    void launcher.loadEmptyView();
    void launcher.initPalette();
    void getVersion()
      .then((v) => (appVersion = v))
      .catch(() => {});
    return () => {
      unlisteners.forEach((u) => u());
      window.removeEventListener("keydown", onWindowKey, true);
    };
  });
</script>

<button type="button" class="backdrop" aria-label={STRINGS.DISMISS_ARIA} onclick={dismiss}></button>
<main class:tall={launcher.settingsOpen || launcher.aboutOpen}>
  {#if launcher.phase === "search"}
    <SearchView
      {launcher}
      {status}
      {showResults}
      {footerKeys}
      exitMs={fadeMs(SEARCH_FADE_MS)}
      onReport={(h: SearchHit) => reportCard(h)}
    />
  {/if}
  {#if launcher.phase === "copied"}
    <CopiedView
      message={launcher.toast}
      inDelay={fadeMs(SEARCH_FADE_MS)}
      duration={fadeMs(COPIED_FADE_MS)}
    />
  {/if}
  {#if launcher.toast && launcher.phase === "search"}
    <Toast message={launcher.toast} />
  {/if}
  {#if launcher.settingsOpen}
    <SettingsPanel onClose={() => launcher.closeSettings()} {launcher} />
  {/if}
  {#if launcher.aboutOpen}
    <AboutPanel
      version={appVersion}
      packs={launcher.packMetas}
      onClose={() => launcher.closeAbout()}
      onToast={(m: string) => launcher.showToast(m)}
    />
  {/if}
</main>
