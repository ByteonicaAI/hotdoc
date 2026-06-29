<script lang="ts">
  import type { Recent, SearchHit } from "./types";
  import { STRINGS } from "./strings";

  type Props = {
    recents: Recent[];
    pinned: SearchHit[];
    onSelectRecent: (query: string) => void;
    onSelectPinned: (hit: SearchHit) => void;
    selectedIndex: number;
    pinnedOffset: number;
    recentsOffset: number;
  };

  const {
    recents = [],
    pinned = [],
    onSelectRecent,
    onSelectPinned,
    selectedIndex,
    pinnedOffset,
    recentsOffset,
  }: Props = $props();

  function recentClick(e: MouseEvent, query: string) {
    e.preventDefault();
    onSelectRecent(query);
  }

  function pinnedClick(e: MouseEvent, hit: SearchHit) {
    e.preventDefault();
    onSelectPinned(hit);
  }

  function pinnedKey(e: KeyboardEvent, hit: SearchHit) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelectPinned(hit);
    }
  }
</script>

{#if recents.length === 0 && pinned.length === 0}
  <li class="empty" aria-hidden="true">{STRINGS.EMPTY_STATE}</li>
{:else}
  {#if pinned.length > 0}
    {#each pinned as p, i (p.id)}
      <li class:active={pinnedOffset + i === selectedIndex} role="presentation">
        <button
          type="button"
          role="option"
          aria-selected={pinnedOffset + i === selectedIndex}
          class="pinned-row"
          onclick={(e) => pinnedClick(e, p)}
          onkeydown={(e) => pinnedKey(e, p)}
          aria-label="{STRINGS.ARIA_PINNED}: {p.title}"
        >
          <span class="pin-mark" aria-hidden="true">📌</span>
          <span class="pinned-title">{p.title}</span>
          <span class="pinned-syntax">{p.syntax}</span>
        </button>
      </li>
    {/each}
  {/if}
  {#if recents.length > 0}
    {#each recents as r, i (r.query)}
      <li class:active={recentsOffset + i === selectedIndex} role="presentation">
        <button
          type="button"
          role="option"
          aria-selected={recentsOffset + i === selectedIndex}
          class="recent-row"
          onclick={(e) => recentClick(e, r.query)}
          aria-label="{STRINGS.ARIA_RECENT_QUERY}: {r.query}"
        >
          <span class="recent-label">{STRINGS.RECENT_LABEL}</span>
          <span class="recent-query">{r.query}</span>
          {#if r.copied_syntax}
            <span class="recent-syntax">{r.copied_syntax}</span>
          {/if}
        </button>
      </li>
    {/each}
  {/if}
{/if}

<style>
  .empty {
    margin: auto 0;
    padding: 28px 16px;
    color: var(--faint);
    font-size: 13px;
    text-align: center;
  }
  /* Selection tint comes from the global `li.active` (accent-soft); the inner
     button stays transparent so the two never double up. */
  .recent-row,
  .pinned-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 2px 4px;
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    text-align: left;
    font: inherit;
  }
  .recent-label {
    font-size: 10px;
    color: var(--faint);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    min-width: 50px;
  }
  .recent-query {
    font-family: var(--mono, monospace);
  }
  .recent-syntax {
    font-family: var(--mono, monospace);
    color: var(--muted, #888);
    font-size: 12px;
    margin-left: auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 40%;
  }
  .pin-mark {
    font-size: 11px;
  }
  .pinned-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pinned-syntax {
    font-family: var(--mono, monospace);
    color: var(--muted, #888);
    font-size: 12px;
  }
  li {
    display: flex;
    align-items: center;
  }
  li > .recent-row,
  li > .pinned-row {
    flex: 1 1 auto;
    min-width: 0;
  }
</style>
