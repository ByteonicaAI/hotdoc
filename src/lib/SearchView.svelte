<script lang="ts">
  import { fade } from "svelte/transition";
  import { prefersReducedMotion } from "svelte/motion";
  import EmptyView from "./EmptyView.svelte";
  import ResultItem from "./ResultItem.svelte";
  import DetailsPane from "./DetailsPane.svelte";
  import type { Launcher } from "./useLauncher.svelte";
  import type { SearchHit } from "./types";
  import { STRINGS, format } from "./strings";

  interface Props {
    launcher: Launcher;
    /** Index status for the footer count, or null while still indexing. */
    status: { entry_count: number; pack_count: number } | null;
    /** Whether to show the results list + footer (vs. just the search bar). */
    showResults: boolean;
    /** Footer key-hint chips. */
    footerKeys: string[];
    /** Exit fade duration (ms) for the Enter-to-copy dismiss; 0 = reduced motion. */
    exitMs: number;
    onReport: (hit: SearchHit) => void;
  }
  const { launcher, status, showResults, footerKeys, exitMs, onReport }: Props = $props();

  // Internal fade for the results list appearing/collapsing (independent of the
  // dismiss choreography's exit fade on the whole view).
  const resultsMs = $derived(prefersReducedMotion.current ? 0 : 140);
</script>

<!-- Wraps input + results + footer so the whole launcher fades out as one on
     Enter-to-copy. The {#if} that mounts this lives in App.svelte; unmounting it
     plays this root element's out:fade. -->
<div class="search-view" out:fade={{ duration: exitMs }}>
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
  {#if showResults}
    <div class="results-area" transition:fade={{ duration: resultsMs }}>
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
              onReport={(h: SearchHit) => onReport(h)}
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
  {/if}
  {#if showResults}
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
  {/if}
</div>

<style>
  /* Inherits main's column layout; fills it in the tall (settings/about) states.
     `main` lives in App.svelte, so the tall selector must reach out of scope. */
  .search-view {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  :global(main.tall) .search-view {
    height: 100%;
  }
  .results-area {
    position: relative;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  /* Size the list to its content so the window grows row by row, capped so it
     shows roughly 5 results then scrolls. Content-sized (not flex-grow) so it
     never collapses to a single row in the auto-height window. */
  .results-area > ul {
    /* Fits ~5 rich rows (syntax + title + desc + example ≈ 110-130px each)
       within the fixed window height, then scrolls. */
    max-height: 580px;
    overflow-y: auto;
  }
  /* The settings/about panel fixes the window height; let the list fill it. */
  :global(main.tall) .results-area > ul {
    max-height: none;
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
