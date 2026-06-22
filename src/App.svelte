<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import ResultItem from "./lib/ResultItem.svelte";
  import EmptyView from "./lib/EmptyView.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import { Launcher } from "./lib/useLauncher.svelte";
  import { getAllSettings, indexStatus } from "./lib/tauri";
  import type { SearchHit } from "./lib/types";
  import { STRINGS } from "./lib/strings";

  const launcher = new Launcher();
  // ponytail: FR-I4 — footer index state. Cold indexing is synchronous in
  // Rust before this window paints, so a streaming N/N counter would never
  // render; the steady-state count is the honest signal. `null` = not yet
  // loaded → "Indexing…".
  let status = $state<{ entry_count: number; pack_count: number } | null>(null);

  onMount(() => {
    document.getElementById("q")?.focus();
    let unlisten: (() => void) | undefined;
    void indexStatus()
      .then((s) => (status = s))
      .catch(() => {
        /* leave as Indexing… */
      });
    void listen("hotdoc://refresh-empty-view", () => {
      void launcher.loadEmptyView();
    }).then((u) => {
      unlisten = u;
    });
    void listen("hotdoc://open-settings", () => {
      launcher.openSettings();
    });
    void listen("hotdoc://reload-index", () => {
      // ponytail: T13 + T16. Tray menu item → IPC rebuild →
      // reloadIndex() in the launcher toasts the count and refreshes
      // the empty view. Errors propagate to the toast pipeline.
      void launcher.reloadIndex();
      void indexStatus()
        .then((s) => (status = s))
        .catch(() => {});
    });
    void listen("hotdoc://copy-diagnostics", () => {
      // ponytail: FR-G2 — tray "Copy diagnostics" → redacted bundle to
      // clipboard, toast confirms.
      void launcher.copyDiagnostics();
    });
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
    return () => unlisten?.();
  });
</script>

<main>
  <input
    id="q"
    placeholder={STRINGS.SEARCH_PLACEHOLDER}
    value={launcher.query}
    oninput={(e) => launcher.onInput(e.currentTarget.value)}
    onkeydown={(e) => launcher.onKey(e)}
    autocomplete="off"
    autocorrect="off"
    spellcheck="false"
  />
  <ul role="listbox" aria-label={STRINGS.SEARCH_RESULTS_ARIA}>
    {#if launcher.emptyQuery}
      <EmptyView
        recents={launcher.recentList}
        pinned={launcher.pinnedList}
        popular={launcher.popularList}
        onSelectRecent={(q: string) => launcher.selectRecent(q)}
        onSelectPinned={(h: SearchHit) => launcher.selectPinned(h)}
        onSelectPopular={(h: SearchHit) => launcher.selectPinned(h)}
        selectedIndex={launcher.selectedIndex}
        pinnedOffset={launcher.recentList.length}
        popularOffset={launcher.recentList.length + launcher.pinnedList.length}
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
  {#if launcher.toast}
    <div class="toast" role="status">{launcher.toast}</div>
  {/if}
  {#if launcher.settingsOpen}
    <SettingsPanel onClose={() => launcher.closeSettings()} {launcher} />
  {/if}
  <footer class="status" aria-live="polite">
    {#if status}
      {status.entry_count} commands · {status.pack_count} packs
    {:else}
      {STRINGS.INDEXING_STATUS}
    {/if}
  </footer>
</main>

<style>
  .status {
    padding: 6px 16px;
    border-top: 1px solid var(--row-active-bg, rgba(255, 255, 255, 0.08));
    color: var(--muted, #888);
    font-size: 11px;
  }
  .zero {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .suggest-label {
    color: var(--muted, #888);
    font-size: 12px;
  }
  .suggest-chips {
    display: inline-flex;
    gap: 6px;
  }
  .suggest-chip {
    font: inherit;
    font-size: 12px;
    padding: 2px 10px;
    border: 1px solid var(--row-active-bg, rgba(255, 255, 255, 0.15));
    border-radius: 999px;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .suggest-chip:hover {
    background: var(--row-active-bg, rgba(255, 255, 255, 0.08));
  }
</style>
