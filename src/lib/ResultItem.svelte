<script lang="ts">
  import { sourceLabel, type SearchHit } from "./types";

  type Props = { hit: SearchHit; active: boolean };
  const { hit, active }: Props = $props();
</script>

<li class:active role="option" aria-selected={active}>
  <div class="row syntax-row">
    <span class="syntax">{hit.syntax}</span>
    <span class="source {hit.source}">{sourceLabel(hit.source)}</span>
  </div>
  <div class="row meta-row">
    <span class="title">{hit.title}</span>
    <span class="pack">{hit.pack_id}</span>
  </div>
  <div class="desc">{hit.description}</div>
  {#if hit.example_code}
    <div class="example"><code>{hit.example_code}</code></div>
  {/if}
</li>
