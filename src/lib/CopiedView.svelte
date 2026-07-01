<script lang="ts">
  import { fade } from "svelte/transition";

  interface Props {
    /** Confirmation text (the launcher's copy toast message). */
    message: string | null;
    /** Delay before fading in, so the search view finishes fading out first. */
    inDelay: number;
    /** Fade in/out duration. */
    duration: number;
  }
  const { message, inDelay, duration }: Props = $props();
</script>

<!-- The "Copied" confirmation shown after the search view fades out, during the
     Enter-to-copy dismiss choreography. -->
<div
  class="copied-view"
  role="status"
  in:fade={{ delay: inDelay, duration }}
  out:fade={{ duration }}
>
  {message}
</div>

<style>
  /* Sits inside main's rounded surface, so it just needs centered themed text. */
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
</style>
