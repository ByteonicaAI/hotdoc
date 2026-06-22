<script lang="ts">
  import { sourceLabel, type SearchHit } from "./types";
  import { highlight } from "./launcher/highlight";

  type Props = {
    hit: SearchHit;
    active: boolean;
    pinned?: boolean;
    query?: string;
    onCopyExample?: (hit: SearchHit) => void;
    onCopyAll?: (hit: SearchHit) => void;
    onTogglePin?: (hit: SearchHit) => void;
    onOpenSource?: (hit: SearchHit) => void;
  };
  const {
    hit,
    active,
    pinned = false,
    query = "",
    onCopyExample,
    onCopyAll,
    onTogglePin,
    onOpenSource,
  }: Props = $props();

  // ponytail: §7.3 — escape-safe highlight. Each segment's text is
  // rendered through Svelte's default escaping ({seg.text}); only the
  // <mark> wrapper is markup. No {@html}, so injected markup in card
  // content stays inert (SEC-2).
  const descSegments = $derived(highlight(hit.description, query));

  // ponytail: hover actions must not steal the input's keybinds. Buttons
  // use onmousedown+preventDefault so the input keeps focus and the
  // launcher doesn't blur-hide before the click handler runs.
  function act(e: Event, fn?: (h: SearchHit) => void) {
    e.preventDefault();
    e.stopPropagation();
    fn?.(hit);
  }
</script>

<li class:active role="option" aria-selected={active}>
  <div class="row syntax-row">
    <span class="syntax">{hit.syntax}</span>
    {#if pinned}
      <span class="pin-mark" aria-label="pinned">📌</span>
    {/if}
    <span class="source {hit.source}">{sourceLabel(hit.source)}</span>
  </div>
  <div class="row meta-row">
    <span class="title">{hit.title}</span>
    <span class="pack">{hit.pack_id}</span>
  </div>
  <div class="desc">
    {#each descSegments as seg}{#if seg.mark}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}
  </div>
  {#if hit.example_code}
    <div class="example"><code>{hit.example_code}</code></div>
  {/if}
  <!-- ponytail: FR-C5 secondary actions; revealed on hover/focus-within. -->
  <div class="actions" aria-label="Card actions">
    <button type="button" onmousedown={(e) => act(e, onCopyExample)}>Copy example</button>
    <button type="button" onmousedown={(e) => act(e, onCopyAll)}>Copy all</button>
    <button type="button" onmousedown={(e) => act(e, onTogglePin)}>
      {pinned ? "Unpin" : "Pin"}
    </button>
    {#if hit.source_url}
      <button type="button" onmousedown={(e) => act(e, onOpenSource)}>Open source</button>
    {/if}
  </div>
</li>

<style>
  .pin-mark {
    font-size: 11px;
  }
  mark {
    background: var(--mark-bg, rgba(250, 204, 21, 0.35));
    color: inherit;
    border-radius: 2px;
  }
  .actions {
    display: none;
    gap: 8px;
    margin-top: 6px;
  }
  li:hover > .actions,
  li:focus-within > .actions {
    display: flex;
  }
  .actions button {
    font: inherit;
    font-size: 11px;
    padding: 2px 8px;
    border: 1px solid var(--row-active-bg, rgba(255, 255, 255, 0.15));
    border-radius: 6px;
    background: transparent;
    color: var(--muted, #888);
    cursor: pointer;
  }
  .actions button:hover {
    color: inherit;
  }
</style>
