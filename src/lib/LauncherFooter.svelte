<script lang="ts">
  import { STRINGS, format } from "./strings";
  import type { IndexStatus } from "./tauri";

  type Props = {
    emptyQuery: boolean;
    resultsEmpty: boolean;
    status: IndexStatus | null;
  };
  const { emptyQuery, resultsEmpty, status }: Props = $props();

  const keys = $derived(
    (emptyQuery || resultsEmpty ? STRINGS.FOOTER_KEYS_EMPTY : STRINGS.FOOTER_KEYS_SEARCH)
      .split("·")
      .map((s) => s.trim())
      .filter(Boolean),
  );
</script>

<footer class="status" aria-live="polite">
  <span class="footer-keys">
    {#each keys as hint (hint)}
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

<style>
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
</style>
