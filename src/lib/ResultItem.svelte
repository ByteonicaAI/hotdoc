<script lang="ts">
  import { sourceLabel, type SearchHit } from "./types";
  import { highlight } from "./launcher/highlight";

  type Props = {
    hit: SearchHit;
    active: boolean;
    pinned?: boolean;
    query?: string;
    id?: string;
  };
  const { hit, active, pinned = false, query = "", id }: Props = $props();

  // ponytail: §7.3 — escape-safe highlight. Each segment's text is
  // rendered through Svelte's default escaping ({seg.text}); only the
  // <mark> wrapper is markup. No {@html}, so injected markup in card
  // content stays inert (SEC-2).
  const descSegments = $derived(highlight(hit.description, query));
</script>

<li {id} class:active role="option" aria-selected={active}>
  <div class="row syntax-row">
    <span class="syntax">{hit.syntax}</span>
    {#if active}
      <span class="row-keys" aria-hidden="true">
        <span class="kbd">↵</span>
        {#if hit.example_code}<span class="kbd">⇧↵</span>{/if}
      </span>
    {/if}
    {#if pinned}
      <span class="pin-mark" aria-hidden="true">📌</span>
    {/if}
    <span class="source {hit.source}">{sourceLabel(hit.source)}</span>
  </div>
  <div class="row meta-row">
    <span class="title">{hit.title}</span>
    <span class="pack">{hit.pack_id}</span>
  </div>
  <div class="desc">
    {#each descSegments as seg, i (i)}{#if seg.mark}<mark>{seg.text}</mark
        >{:else}{seg.text}{/if}{/each}
  </div>
  {#if hit.example_code}
    <div class="example"><code>{hit.example_code}</code></div>
  {/if}
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
</style>
