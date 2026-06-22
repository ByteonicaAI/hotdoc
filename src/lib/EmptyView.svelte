<script lang="ts">
  import type { Recent, SearchHit } from "./types";
  import { STRINGS } from "./strings";

  type Props = {
    recents: Recent[];
    pinned: SearchHit[];
    popular?: SearchHit[];
    onSelectRecent: (query: string) => void;
    onSelectPinned: (hit: SearchHit) => void;
    onSelectPopular?: (hit: SearchHit) => void;
    selectedIndex: number;
    pinnedOffset: number;
    popularOffset?: number;
  };

  const {
    recents = [],
    pinned = [],
    popular = [],
    onSelectRecent,
    onSelectPinned,
    onSelectPopular,
    selectedIndex,
    pinnedOffset,
    popularOffset = pinnedOffset + pinned.length,
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

  function popularClick(e: MouseEvent, hit: SearchHit) {
    e.preventDefault();
    onSelectPopular?.(hit);
  }

  function popularKey(e: KeyboardEvent, hit: SearchHit) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelectPopular?.(hit);
    }
  }
</script>

{#if recents.length === 0 && pinned.length === 0 && popular.length === 0}
  <li class="empty" aria-hidden="true">{STRINGS.EMPTY_STATE}</li>
{:else}
  {#if recents.length > 0}
    {#each recents as r, i (r.query)}
      <li class:active={i === selectedIndex} role="presentation">
        <button
          type="button"
          role="option"
          aria-selected={i === selectedIndex}
          class="recent-row"
          onclick={(e) => recentClick(e, r.query)}
          aria-label="Recent query: {r.query}"
        >
          <span class="recent-label">{STRINGS.RECENT_LABEL}</span>
          <span class="recent-query">{r.query}</span>
        </button>
      </li>
    {/each}
  {/if}
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
          aria-label="Pinned: {p.title}"
        >
          <span class="pin-mark" aria-hidden="true">📌</span>
          <span class="pinned-title">{p.title}</span>
          <span class="pinned-syntax">{p.syntax}</span>
        </button>
      </li>
    {/each}
  {/if}
  {#if popular.length > 0}
    <li class="section-label" aria-hidden="true">{STRINGS.POPULAR_LABEL}</li>
    {#each popular as p, i (p.id)}
      <li class:active={popularOffset + i === selectedIndex} role="presentation">
        <button
          type="button"
          role="option"
          aria-selected={popularOffset + i === selectedIndex}
          class="pinned-row"
          onclick={(e) => popularClick(e, p)}
          onkeydown={(e) => popularKey(e, p)}
          aria-label="Popular: {p.title}"
        >
          <span class="popular-title">{p.title}</span>
          <span class="pinned-syntax">{p.syntax}</span>
        </button>
      </li>
    {/each}
  {/if}
{/if}

<style>
  .empty {
    padding: 12px 16px;
    color: var(--muted, #888);
    font-size: 13px;
  }
  .section-label {
    padding: 6px 16px 2px;
    color: var(--muted, #888);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .popular-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .recent-row,
  .pinned-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 16px;
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    text-align: left;
    font: inherit;
  }
  li.active > .recent-row,
  li.active > .pinned-row {
    background: var(--row-active-bg, rgba(255, 255, 255, 0.08));
  }
  .recent-label {
    font-size: 11px;
    color: var(--muted, #888);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    min-width: 56px;
  }
  .recent-query {
    font-family: var(--mono, monospace);
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
</style>
