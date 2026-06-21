<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import ResultItem from "./lib/ResultItem.svelte";
  import EmptyView from "./lib/EmptyView.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import { Launcher } from "./lib/useLauncher.svelte";
  import type { SearchHit } from "./lib/types";

  const launcher = new Launcher();

  onMount(() => {
    document.getElementById("q")?.focus();
    let unlisten: (() => void) | undefined;
    void listen("hotdoc://refresh-empty-view", () => {
      void launcher.loadEmptyView();
    }).then((u) => {
      unlisten = u;
    });
    void listen("hotdoc://open-settings", () => {
      launcher.openSettings();
    });
    void launcher.loadEmptyView();
    void launcher.initPalette();
    return () => unlisten?.();
  });
</script>

<main>
  <input
    id="q"
    placeholder="hotdoc: type to search…"
    value={launcher.query}
    oninput={(e) => launcher.onInput(e.currentTarget.value)}
    onkeydown={(e) => launcher.onKey(e)}
    autocomplete="off"
    autocorrect="off"
    spellcheck="false"
  />
  <ul role="listbox" aria-label="Search results">
    {#if launcher.emptyQuery}
      <EmptyView
        recents={launcher.recentList}
        pinned={launcher.pinnedList}
        onSelectRecent={(q: string) => launcher.selectRecent(q)}
        onSelectPinned={(h: SearchHit) => launcher.selectPinned(h)}
        selectedIndex={-1}
        pinnedOffset={launcher.recentList.length}
      />
    {:else}
      {#each launcher.results as r, i (r.id)}
        <ResultItem
          hit={r}
          active={i === launcher.selectedIndex}
          pinned={launcher.pinnedIds.has(r.id)}
        />
      {/each}
      {#if launcher.zeroResult}
        <li class="empty" aria-hidden="true">No matches for "{launcher.query.trim()}"</li>
      {/if}
    {/if}
  </ul>
  {#if launcher.toast}
    <div class="toast" role="status">{launcher.toast}</div>
  {/if}
  {#if launcher.settingsOpen}
    <SettingsPanel onClose={() => launcher.closeSettings()} />
  {/if}
</main>
