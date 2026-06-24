<script lang="ts">
  import type { SearchHit } from "./types";
  import { sourceLabel } from "./types";
  import { openUrl } from "./tauri";
  import { STRINGS } from "./strings";
  import { isHttpsUrl } from "./url";

  type Props = {
    hit: SearchHit;
    onClose: () => void;
    onToast: (msg: string) => void;
  };
  const { hit, onClose, onToast }: Props = $props();

  // ponytail: M4.5-T4 / SEC-3 — add the same frontend isHttpsUrl
  // guard the launcher's activate() already does, so the user sees
  // a toast when the URL is non-https instead of an open call that
  // the Rust gate silently rejects.
  async function handleSourceLink(e: MouseEvent) {
    e.preventDefault();
    if (!hit.source_url) return;
    if (!isHttpsUrl(hit.source_url)) {
      onToast(STRINGS.TOAST_OPEN_REJECTED);
      return;
    }
    try {
      await openUrl(hit.source_url);
    } catch (e) {
      onToast(STRINGS.TOAST_OPEN_FAIL.replace("$1", String(e)));
    }
  }
</script>

<section aria-label={STRINGS.DETAILS_ARIA} data-testid="details-pane" class="details-pane">
  <div class="details-header">
    <span class="details-title">{STRINGS.DETAILS_TITLE}</span>
    <button type="button" class="close-btn" aria-label={STRINGS.CLOSE_ARIA} onclick={onClose}>
      ✕
    </button>
  </div>
  <div class="details-body">
    <div class="detail-row">
      <span class="detail-label">{STRINGS.DETAILS_COMMAND_LABEL}</span>
      <code class="detail-syntax">{hit.syntax}</code>
    </div>
    <div class="detail-row">
      <span class="detail-label">{STRINGS.DETAILS_PACK_LABEL}</span>
      <span>{hit.pack_id}</span>
    </div>
    <div class="detail-row">
      <span class="detail-label">{STRINGS.DETAILS_SOURCE_LABEL}</span>
      <span class="source {hit.source}">{sourceLabel(hit.source)}</span>
    </div>
    {#if hit.description}
      <div class="detail-desc">{hit.description}</div>
    {/if}
    {#if hit.example_code}
      <div class="detail-section-label">{STRINGS.DETAILS_EXAMPLE_LABEL}</div>
      <pre class="detail-example"><code>{hit.example_code}</code></pre>
    {/if}
    {#if hit.source_url}
      <div class="detail-source-row">
        <button type="button" class="source-link" onclick={handleSourceLink}>
          {hit.source_url}
        </button>
      </div>
    {/if}
  </div>
</section>

<style>
  .details-pane {
    border-top: 1px solid var(--border, #e5e7eb);
    padding: 10px 12px;
    background: var(--bg);
    overflow-y: auto;
    max-height: 200px;
  }
  .details-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .details-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted, #888);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .close-btn {
    background: transparent;
    border: none;
    color: var(--muted, #888);
    cursor: pointer;
    font-size: 12px;
    padding: 2px 6px;
    border-radius: 4px;
    line-height: 1;
  }
  .close-btn:hover {
    color: var(--fg);
  }
  .details-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .detail-row {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .detail-label {
    font-size: 11px;
    color: var(--muted, #888);
    min-width: 60px;
    flex: 0 0 auto;
  }
  .detail-syntax {
    font-family: var(--mono, monospace);
    font-size: 13px;
  }
  .source {
    font-size: 11px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 4px;
    color: white;
    background: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }
  .source.official {
    background: var(--source-official);
  }
  .source.cheat-sheet {
    background: var(--source-cheat-sheet);
  }
  .source.curated {
    background: var(--source-curated);
  }
  .detail-desc {
    font-size: 12px;
    color: var(--muted, #888);
    line-height: 1.5;
  }
  .detail-section-label {
    font-size: 11px;
    color: var(--muted, #888);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-top: 4px;
  }
  .detail-example {
    font-family: var(--mono, monospace);
    font-size: 12px;
    background: var(--active-bg, #f4f4f5);
    padding: 6px 10px;
    border-radius: 6px;
    margin: 0;
    overflow-x: auto;
    white-space: pre-wrap;
  }
  .detail-source-row {
    margin-top: 4px;
  }
  .source-link {
    background: transparent;
    border: none;
    color: var(--source-official, #2563eb);
    cursor: pointer;
    font-size: 11px;
    font-family: var(--mono, monospace);
    padding: 0;
    text-align: left;
    text-decoration: underline;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .source-link:hover {
    opacity: 0.8;
  }
</style>
