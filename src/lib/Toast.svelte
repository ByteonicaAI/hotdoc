<script lang="ts">
  interface Props {
    /** Message text to show in the toast. */
    message: string;
  }
  const { message }: Props = $props();
</script>

<!-- Transient confirmation/error pill (copy from a button, pin, reload, etc.).
     The Enter-to-copy path uses CopiedView instead; this is the in-place toast
     that leaves the launcher open. -->
<div class="toast" role="status">{message}</div>

<style>
  /* Themed pill so it reads as part of the surface in both light and dark. */
  .toast {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    padding: 7px 13px;
    border-radius: 7px;
    font-size: 12px;
    font-family: var(--font-mono);
    white-space: nowrap;
    max-width: calc(100% - 32px);
    overflow: hidden;
    text-overflow: ellipsis;
    box-shadow: var(--shadow);
    animation: toast-in 130ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateX(-50%) translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateX(-50%) translateY(0);
    }
  }
</style>
