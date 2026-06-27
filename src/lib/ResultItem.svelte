<script lang="ts">
  import { sourceLabel, type SearchHit } from "./types";
  import { highlight } from "./launcher/highlight";
  import { STRINGS } from "./strings";

  type Props = {
    hit: SearchHit;
    active: boolean;
    pinned?: boolean;
    query?: string;
    onCopyExample?: (hit: SearchHit) => void;
    onCopyAll?: (hit: SearchHit) => void;
    onTogglePin?: (hit: SearchHit) => void;
    onOpenSource?: (hit: SearchHit) => void;
    onReport?: (hit: SearchHit) => void;
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
    onReport,
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
    {#if active}
      <span class="row-keys" aria-hidden="true">
        <span class="kbd">↵</span>
        {#if hit.example_code}<span class="kbd">⇧↵</span>{/if}
      </span>
    {/if}
    {#if pinned}
      <span class="pin-mark" aria-label={STRINGS.PIN_ARIA} aria-hidden="true">📌</span>
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
  <!-- ponytail: FR-C5 secondary actions; revealed on hover/focus-within.
       aria-hidden: primary keyboard flows use the input's key handler (Enter/Shift+Enter/
       Ctrl+P/Ctrl+Enter). These hover buttons are visual shortcuts; AT users activate
       via keyboard shortcuts instead (NFR-9). -->
  <div class="actions" aria-label={STRINGS.CARD_ACTIONS_ARIA} aria-hidden="true">
    <button type="button" onmousedown={(e) => act(e, onCopyExample)}
      >{STRINGS.COPY_EXAMPLE_BTN}</button
    >
    <button type="button" onmousedown={(e) => act(e, onCopyAll)}>{STRINGS.COPY_ALL_BTN}</button>
    <button type="button" onmousedown={(e) => act(e, onTogglePin)}>
      {pinned ? STRINGS.UNPIN_BTN : STRINGS.PIN_BTN}
    </button>
    {#if hit.source_url}
      <button type="button" onmousedown={(e) => act(e, onOpenSource)}
        >{STRINGS.OPEN_SOURCE_BTN}</button
      >
    {/if}
    <button type="button" onmousedown={(e) => act(e, onReport)}>{STRINGS.REPORT_CARD_BTN}</button>
  </div>
</li>

<style>
  .pin-mark {
    flex: 0 0 auto;
    font-size: 11px;
  }
  .row-keys {
    flex: 0 0 auto;
    display: inline-flex;
    gap: 4px;
  }
  mark {
    background: var(--mark-bg, rgba(250, 204, 21, 0.35));
    color: inherit;
    border-radius: 3px;
    padding: 0 1px;
  }
  /* Mouse users get the same actions on hover; keyboard is primary. Hidden
     until the row is hovered or focused so it never competes with the read. */
  .actions {
    display: flex;
    opacity: 0;
    pointer-events: none;
    gap: 6px;
    margin-top: 5px;
    transition: opacity 90ms ease;
  }
  li:hover > .actions,
  li:focus-within > .actions {
    opacity: 1;
    pointer-events: auto;
  }
  .actions button {
    font: 500 11px/1 var(--font-ui);
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--muted, #888);
    cursor: pointer;
    transition: border-color 90ms ease;
  }
  .actions button:hover {
    color: var(--fg);
    border-color: var(--accent);
  }
</style>
