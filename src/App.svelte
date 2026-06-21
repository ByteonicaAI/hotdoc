<script lang="ts">
  import { onMount } from "svelte";
  import ResultItem from "./lib/ResultItem.svelte";
  import { Launcher } from "./lib/useLauncher.svelte";

  const launcher = new Launcher();

  onMount(() => {
    document.getElementById("q")?.focus();
  });
</script>

<main>
  <input
    id="q"
    placeholder="hotdoc: type to search…"
    oninput={(e) => launcher.onInput(e.currentTarget.value)}
    onkeydown={(e) => launcher.onKey(e)}
    bind:value={launcher.query}
    autocomplete="off"
    autocorrect="off"
    spellcheck="false"
  />
  <ul role="listbox" aria-label="Search results">
    {#each launcher.results as r, i (r.id)}
      <ResultItem hit={r} active={i === 0} />
    {/each}
    {#if launcher.zeroResult}
      <li class="empty" aria-hidden="true">No matches for "{launcher.query.trim()}"</li>
    {/if}
  </ul>
  {#if launcher.toast}
    <div class="toast" role="status">{launcher.toast}</div>
  {/if}
</main>
