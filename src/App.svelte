<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getVersion } from "@tauri-apps/api/app";
  import ResultItem from "./lib/ResultItem.svelte";
  import EmptyView from "./lib/EmptyView.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import AboutPanel from "./lib/AboutPanel.svelte";
  import DetailsPane from "./lib/DetailsPane.svelte";
  import { Launcher } from "./lib/useLauncher.svelte";
  import { getAllSettings, indexStatus, openUrl } from "./lib/tauri";
  import type { SearchHit } from "./lib/types";
  import { STRINGS, format } from "./lib/strings";

  const launcher = new Launcher();

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

<main>
  <div class="input-zone">
    <span class="input-glyph" aria-hidden="true">
      <svg
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <circle cx="11" cy="11" r="7" />
        <line x1="21" y1="21" x2="16.5" y2="16.5" />
      </svg>
    </span>
    <input
      id="q"
      aria-label={STRINGS.SEARCH_INPUT_ARIA}
      placeholder={STRINGS.SEARCH_PLACEHOLDER}
      bind:value={launcher.query}
      oninput={(e) => launcher.onInput(e.currentTarget.value)}
      onkeydown={(e) => launcher.onKey(e)}
      autocomplete="off"
      autocorrect="off"
      spellcheck="false"
    />
    {#if !launcher.emptyQuery && launcher.results.length > 0}
      <span class="input-count" aria-hidden="true">{launcher.results.length}</span>
    {/if}
  </div>
  <div class="results-area">
    <ul role="listbox" aria-label={STRINGS.SEARCH_RESULTS_ARIA}>
      {#if launcher.emptyQuery}
        <EmptyView
          recents={launcher.recentList}
          pinned={launcher.pinnedList}
          onSelectRecent={(q: string) => launcher.selectRecent(q)}
          onSelectPinned={(h: SearchHit) => launcher.selectPinned(h)}
          onCopyText={(text: string) => launcher.copyText(text)}
          selectedIndex={launcher.selectedIndex}
          pinnedOffset={0}
          recentsOffset={launcher.pinnedList.length}
        />
      {:else}
        {#each launcher.results as r, i (r.id)}
          <ResultItem
            hit={r}
            active={i === launcher.selectedIndex}
            pinned={launcher.pinnedIds.has(r.id)}
            query={launcher.query}
            onCopyExample={(h: SearchHit) => launcher.copyExample(h)}
            onCopyAll={(h: SearchHit) => launcher.copyAll(h)}
            onTogglePin={(h: SearchHit) => launcher.togglePinHit(h)}
            onOpenSource={(h: SearchHit) => launcher.openSource(h)}
            onReport={(h: SearchHit) => reportCard(h)}
          />
        {/each}
        {#if launcher.zeroResult}
          <li class="empty zero" role="status">
            {STRINGS.NO_MATCHES_PREFIX}{launcher.query.trim()}{STRINGS.NO_MATCHES_SUFFIX}
            {#if launcher.suggestions.length > 0}
              <span class="suggest-label">{STRINGS.SUGGEST_PREFIX}</span>
              <span class="suggest-chips">
                {#each launcher.suggestions as pack (pack)}
                  <button
                    type="button"
                    class="suggest-chip"
                    onmousedown={(e) => {
                      e.preventDefault();
                      launcher.applySuggestion(pack);
                    }}
                  >
                    {pack}
                  </button>
                {/each}
              </span>
            {/if}
          </li>
        {/if}
      {/if}
    </ul>
    {#if launcher.detailsHit}
      <DetailsPane
        hit={launcher.detailsHit}
        onClose={() => launcher.closeDetails()}
        onToast={(m: string) => launcher.showToast(m)}
      />
    {/if}
  </div>
  {#if launcher.toast}
    <div class="toast" role="status">{launcher.toast}</div>
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
  <footer class="status" aria-live="polite">
    <span class="footer-keys">
      {#each footerKeys as hint (hint)}
        <span class="hint">{hint}</span>
      {/each}
    </span>
    <span class="footer-count">
      {#if status}
        {format(STRINGS.FOOTER_STATUS, String(status.entry_count), String(status.pack_count))}
      {:else}
        {STRINGS.INDEXING_STATUS}
      {/if}
    </span>
  </footer>
</main>

<style>
  .results-area {
    position: relative;
    flex: 1 1 auto;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .results-area > ul {
    flex: 1 1 auto;
    overflow-y: auto;
  }
  .status {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 14px;
    border-top: 1px solid var(--border);
  }
  .footer-keys {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
    overflow: hidden;
  }
  .footer-keys .hint {
    flex: 0 0 auto;
    color: var(--muted);
    font-size: 11px;
    white-space: nowrap;
  }
  .footer-count {
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 11px;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  /* Zero-result lives as a centered block, not a row in the list. */
  .zero {
    /* The only row when search fails; center it in the list's free space. */
    margin: auto 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 28px 16px;
    text-align: center;
  }
  .suggest-label {
    color: var(--faint);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .suggest-chips {
    display: inline-flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
  }
  .suggest-chip {
    font: 500 12px/1 var(--font-mono);
    padding: 5px 11px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--active-bg);
    color: var(--fg);
    cursor: pointer;
    transition: border-color 90ms ease;
  }
  .suggest-chip:hover {
    border-color: var(--accent);
  }
</style>
