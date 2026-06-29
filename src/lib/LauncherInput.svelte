<script lang="ts">
  import { STRINGS } from "./strings";

  type Props = {
    query: string;
    onInput: (value: string) => void;
    onKey: (e: KeyboardEvent) => void;
    resultCount: number;
  };
  // eslint-disable-next-line prefer-const
  let { query = $bindable(""), onInput, onKey, resultCount }: Props = $props();
</script>

<div class="input-zone">
  <span class="input-glyph" aria-hidden="true">
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <circle cx="11" cy="11" r="7" />
      <line x1="21" y1="21" x2="16.5" y2="16.5" />
    </svg>
  </span>
  <input
    id="q"
    aria-label={STRINGS.SEARCH_INPUT_ARIA}
    placeholder={STRINGS.SEARCH_PLACEHOLDER}
    bind:value={query}
    oninput={(e) => onInput(e.currentTarget.value)}
    onkeydown={(e) => onKey(e)}
    autocomplete="off"
    autocorrect="off"
    spellcheck="false"
  />
  {#if resultCount > 0}
    <span class="input-count" aria-hidden="true">{resultCount}</span>
  {/if}
</div>

<style>
  .input-zone {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
  }
  .input-glyph {
    color: var(--muted);
    flex: 0 0 auto;
  }
  input {
    flex: 1 1 auto;
    background: transparent;
    border: none;
    color: var(--fg);
    font: 500 14px/1.4 var(--font-ui);
    outline: none;
  }
  input::placeholder {
    color: var(--faint);
  }
  .input-count {
    flex: 0 0 auto;
    color: var(--muted);
    font: 500 11px/1 var(--font-mono);
  }
</style>
