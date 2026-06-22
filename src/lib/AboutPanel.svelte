<script lang="ts">
  import type { PackMeta } from "./types";
  import { STRINGS } from "./strings";

  type Props = {
    version: string;
    packs: PackMeta[];
    onClose: () => void;
  };
  const { version, packs, onClose }: Props = $props();
</script>

<div class="about" role="dialog" aria-label={STRINGS.ABOUT_ARIA} data-testid="about-panel">
  <header>
    <h2>{STRINGS.ABOUT_TITLE}</h2>
    <button type="button" class="close" onclick={onClose} aria-label={STRINGS.CLOSE_ARIA}>×</button>
  </header>

  <section>
    <div class="meta-row">
      <span class="label">Version</span>
      <span>{version}</span>
    </div>
    <div class="meta-row">
      <span class="label">App license</span>
      <span>{STRINGS.ABOUT_LICENSE}</span>
    </div>
    <div class="meta-row">
      <span class="label">Content license</span>
      <span>{STRINGS.ABOUT_CONTENT_LICENSE}</span>
    </div>
  </section>

  {#if packs.length > 0}
    <section>
      <span class="section-label">Attribution</span>
      <table>
        <thead>
          <tr>
            <th>Pack</th>
            <th>License</th>
            <th>Homepage</th>
          </tr>
        </thead>
        <tbody>
          {#each packs as p (p.id)}
            <tr>
              <td>{p.name} ({p.id})</td>
              <td>{p.license}</td>
              <td>
                {#if p.homepage}
                  <a href={p.homepage} target="_blank" rel="noreferrer">{p.homepage}</a>
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
  a {
    color: var(--accent, #4a9eff);
    text-decoration: none;
    word-break: break-all;
  }
  a:hover {
    text-decoration: underline;
  }
</style>
