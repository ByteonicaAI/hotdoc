<script lang="ts">
  import type { PackMeta } from "./types";
  import { STRINGS } from "./strings";
  import { openUrl } from "./tauri";
  import { isHttpsUrl } from "./url";

  type Props = {
    version: string;
    packs: PackMeta[];
    onClose: () => void;
    onToast: (msg: string) => void;
  };
  const { version, packs, onClose, onToast }: Props = $props();

  // ponytail: M4.5-T4 / SEC-3 — all URL opens route through the Rust
  // open_url IPC (HTTPS-only gate). The native <a target="_blank">
  // bypassed the gate; a pack could declare homepage: "javascript:..."
  // or "http://evil.com" and open directly. This handler mirrors
  // the FastPath in useLauncher's activate() — reject-then-toast.
  async function handleHomepage(homepage: string) {
    if (!isHttpsUrl(homepage)) {
      onToast(STRINGS.TOAST_OPEN_REJECTED);
      return;
    }
    try {
      await openUrl(homepage);
    } catch {
      // Rust gate refused (defense in depth). Mirror the same toast
      // the launcher shows so the user gets the same feedback either way.
      onToast(STRINGS.TOAST_OPEN_REJECTED);
    }
  }
</script>

<div class="about" role="dialog" aria-label={STRINGS.ABOUT_ARIA} data-testid="about-panel">
  <header>
    <h2>{STRINGS.ABOUT_TITLE}</h2>
    <button type="button" class="close" onclick={onClose} aria-label={STRINGS.CLOSE_ARIA}>×</button>
  </header>

  <section>
    <div class="meta-row">
      <span class="label">{STRINGS.ABOUT_VERSION_LABEL}</span>
      <span>{version}</span>
    </div>
    <div class="meta-row">
      <span class="label">{STRINGS.ABOUT_APP_LICENSE_LABEL}</span>
      <span>{STRINGS.ABOUT_LICENSE}</span>
    </div>
    <div class="meta-row">
      <span class="label">{STRINGS.ABOUT_CONTENT_LICENSE_LABEL}</span>
      <span>{STRINGS.ABOUT_CONTENT_LICENSE}</span>
    </div>
  </section>

  {#if packs.length > 0}
    <section>
      <span class="section-label">{STRINGS.ABOUT_ATTRIBUTION_LABEL}</span>
      <table>
        <thead>
          <tr>
            <th>{STRINGS.ABOUT_TABLE_PACK}</th>
            <th>{STRINGS.ABOUT_TABLE_LICENSE}</th>
            <th>{STRINGS.ABOUT_TABLE_HOMEPAGE}</th>
          </tr>
        </thead>
        <tbody>
          {#each packs as p (p.id)}
            <tr>
              <td>{p.name} ({p.id})</td>
              <td>{p.license}</td>
              <td>
                {#if p.homepage}
                  <button
                    type="button"
                    class="homepage-link"
                    data-testid="homepage-link"
                    onclick={() => handleHomepage(p.homepage)}
                  >
                    {p.homepage}
                  </button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</div>

<style>
  .about {
    position: absolute;
    inset: 0;
    background: var(--bg, #1a1a1a);
    color: var(--fg, #eee);
    padding: 16px;
    z-index: 10;
    overflow-y: auto;
    font-size: 13px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
  }
  h2 {
    margin: 0;
    font-size: 14px;
  }
  .close {
    background: transparent;
    border: none;
    color: inherit;
    font-size: 18px;
    cursor: pointer;
  }
  section {
    margin-bottom: 16px;
  }
  .section-label {
    display: block;
    margin-bottom: 6px;
    color: var(--muted, #888);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .meta-row {
    display: flex;
    gap: 12px;
    margin-bottom: 4px;
  }
  .label {
    color: var(--muted, #888);
    min-width: 110px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th,
  td {
    text-align: left;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border, rgba(255, 255, 255, 0.08));
  }
  th {
    color: var(--muted, #888);
    font-weight: 600;
    font-size: 11px;
    text-transform: uppercase;
  }
  .homepage-link {
    color: var(--accent, #4a9eff);
    text-decoration: none;
    word-break: break-all;
    font: inherit;
    background: transparent;
    border: none;
    padding: 0;
    cursor: pointer;
    text-align: left;
  }
  .homepage-link:hover {
    text-decoration: underline;
  }
</style>
