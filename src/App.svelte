<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import ResultItem from "./lib/ResultItem.svelte";
  import EmptyView from "./lib/EmptyView.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import AboutPanel from "./lib/AboutPanel.svelte";
  import DetailsPane from "./lib/DetailsPane.svelte";
  import LauncherInput from "./lib/LauncherInput.svelte";
  import LauncherFooter from "./lib/LauncherFooter.svelte";
  import LauncherCopiedView from "./lib/LauncherCopiedView.svelte";
  import { Launcher } from "./lib/useLauncher.svelte";
  import { setupLauncher, type LifecycleRefs } from "./lib/launcher/lifecycle";
  import type { SearchHit } from "./lib/types";
  import { setWindowSize } from "./lib/tauri";
  import { STRINGS } from "./lib/strings";

  const launcher = new Launcher();
  const refs = $state<LifecycleRefs>({
    status: { current: null },
    appVersion: { current: "" },
  });

  const showResults = $derived(
    !launcher.emptyQuery || launcher.pinnedList.length + launcher.recentList.length > 0,
  );

  // ponytail: WS-F phase 1 fix — window-height sync runs as an inline
  // $effect (not from a Launcher method) so the effect lives in this
  // component's scope and never throws `effect_orphan`. Settings/About
  // force tall (820); everything else stays compact (660). The Rust
  // side (`set_window_size` IPC) handles the actual resize; main.tall +
  // .search-view's CSS height transition mask the snap visually.
  const WINDOW_HEIGHTS = { compact: 660, tall: 820 } as const;
  $effect(() => {
    const target =
      launcher.settingsOpen || launcher.aboutOpen ? WINDOW_HEIGHTS.tall : WINDOW_HEIGHTS.compact;
    void setWindowSize(target);
  });

  $effect(() => {
    const _ = launcher.selectedIndex;
    document.querySelector('[role="listbox"] li.active')?.scrollIntoView({ block: "nearest" });
  });

  onMount(() => {
    return setupLauncher(launcher, refs).cleanup;
  });
</script>

<button
  type="button"
  class="backdrop"
  aria-label={STRINGS.DISMISS_ARIA}
  onclick={() => void launcher.doHide()}
></button>
<main class:tall={launcher.settingsOpen || launcher.aboutOpen}>
  {#if launcher.phase === "search"}
    <div class="search-view" out:fade={{ duration: 140 }}>
      <LauncherInput
        bind:query={launcher.query}
        onInput={(v: string) => launcher.onInput(v)}
        onKey={(e: KeyboardEvent) => launcher.onKey(e)}
        resultCount={launcher.results.length}
      />
      {#if showResults}
        <div class="results-area">
          <ul role="listbox" aria-label={STRINGS.SEARCH_RESULTS_ARIA}>
            {#if launcher.emptyQuery}
              <EmptyView
                recents={launcher.recentList}
                pinned={launcher.pinnedList}
                onSelectRecent={(q: string) => launcher.selectRecent(q)}
                onSelectPinned={(h: SearchHit) => launcher.selectPinned(h)}
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
        <LauncherFooter
          emptyQuery={launcher.emptyQuery}
          resultsEmpty={launcher.results.length === 0}
          status={refs.status.current}
        />
      {/if}
    </div>
  {/if}
  {#if launcher.phase === "copied"}
    <LauncherCopiedView
      message={launcher.toast ?? ""}
      showToast={true}
      searchFadeMs={160}
      copiedFadeMs={200}
    />
  {/if}
  {#if launcher.phase === "search" && launcher.toast}
    <LauncherCopiedView
      message={launcher.toast}
      showToast={false}
      searchFadeMs={0}
      copiedFadeMs={0}
    />
  {/if}
  {#if launcher.settingsOpen}
    <SettingsPanel onClose={() => launcher.closeSettings()} {launcher} />
  {/if}
  {#if launcher.aboutOpen}
    <AboutPanel
      version={refs.appVersion.current}
      packs={launcher.packMetas}
      onClose={() => launcher.closeAbout()}
      onToast={(m: string) => launcher.showToast(m)}
    />
  {/if}
</main>

<style>
  .search-view {
    display: flex;
    flex-direction: column;
    min-height: 0;
    transition: height 140ms cubic-bezier(0.2, 0, 0, 1);
  }
  @media (prefers-reduced-motion: reduce) {
    .search-view {
      transition: none;
    }
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
  .results-area > ul {
    max-height: 580px;
    overflow-y: auto;
  }
  main.tall .results-area > ul {
    max-height: none;
  }
  .zero {
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
