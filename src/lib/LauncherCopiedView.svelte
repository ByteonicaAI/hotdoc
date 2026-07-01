<script lang="ts">
  import { fade } from "svelte/transition";
  import { prefersReducedMotion } from "svelte/motion";

  type Props = {
    message: string;
    showToast: boolean;
    searchFadeMs: number;
    copiedFadeMs: number;
  };
  const { message, showToast, searchFadeMs, copiedFadeMs }: Props = $props();

  function fadeMs(ms: number): number {
    return prefersReducedMotion.current ? 0 : ms;
  }
</script>

{#if showToast}
  <div
    class="copied-view"
    role="status"
    in:fade={{ delay: fadeMs(searchFadeMs), duration: fadeMs(copiedFadeMs) }}
    out:fade={{ duration: fadeMs(copiedFadeMs) }}
  >
    {message}
  </div>
{/if}
{#if message && !showToast}
  <div class="toast" role="status">{message}</div>
{/if}

<style>
  .copied-view {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 16px 18px;
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--fg);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .toast {
    padding: 8px 12px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--fg);
  }
</style>
