<script lang="ts">
  import { onMount } from "svelte";
  import { getAllSettings, setAutostart, setHotkey, setSetting, type SettingsMap } from "./tauri";
  import { Launcher, type Theme } from "./useLauncher.svelte";
  import { log } from "./logger";
  import { STRINGS } from "./strings";

  type Props = { onClose: () => void; launcher?: Launcher };
  const { onClose, launcher: externalLauncher }: Props = $props();

  // ponytail: hotkeyInputEl is bound via bind:this, not bind:value. Using a
  // reactive bind:value schedules a Svelte DOM-update microtask when onMount
  // sets the stored value; that microtask fires after fireEvent.input (in
  // tests) resets element.value back to the stored value, so saveHotkey reads
  // the wrong combo. Reading the live element.value directly avoids the race.
  let hotkeyInputEl: HTMLInputElement | null = null;
  // ponytail: validate the stored theme string against the closed enum
  // (plan-m3.5 §3 T11). A stale "blue" row from a previous schema, a manual
  // DB edit, or a migration that didn't rewrite a value would otherwise
  // un-check every radio. applyTheme also normalises; this normalises the
  // radio state too so the displayed selection matches the applied theme.
  const VALID_THEMES: ReadonlySet<Theme> = new Set<Theme>(["light", "dark", "system"]);
  function normaliseTheme(v: unknown): Theme {
    return VALID_THEMES.has(v as Theme) ? (v as Theme) : "system";
  }
  let theme = $state<Theme>("system");
  let autostart = $state(false);
  let recentsEnabled = $state(true);
  let status = $state<{ kind: "ok" | "err"; msg: string } | null>(null);
  // ponytail: a single Launcher instance is owned by App.svelte. For tests
  // that render SettingsPanel in isolation we fall back to a private one
  // so applyTheme still has somewhere to write — the persist side (setSetting)
  // is what the settings panel is responsible for either way.
  const launcher = new Launcher();

  onMount(async () => {
    try {
      const s: SettingsMap = await getAllSettings();
      if (hotkeyInputEl) {
        hotkeyInputEl.value = s["hotkey"] ?? "Ctrl+Shift+Space";
      }
      theme = normaliseTheme(s["theme"]);
      autostart = s["autostart"] === "true";
      recentsEnabled = s["recents_enabled"] !== "false";
    } catch (e) {
      log.warn("load settings failed", { error: String(e) });
      if (hotkeyInputEl) {
        hotkeyInputEl.value = "Ctrl+Shift+Space";
      }
    }
  });

  async function saveHotkey() {
    status = null;
    const combo = hotkeyInputEl?.value ?? "Ctrl+Shift+Space";
    try {
      await setHotkey(combo);
      await setSetting("hotkey", combo);
      status = { kind: "ok", msg: "Saved." };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function pickTheme(v: Theme) {
    status = null;
    const next = normaliseTheme(v);
    theme = next;
    launcher.applyTheme(next);
    try {
      await setSetting("theme", next);
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function toggleAutostart() {
    status = null;
    const next = !autostart;
    try {
      await setAutostart(next);
      await setSetting("autostart", String(next));
      autostart = next;
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function toggleRecents() {
    status = null;
    const next = !recentsEnabled;
    try {
      await setSetting("recents_enabled", String(next));
      recentsEnabled = next;
      // ponytail: T19 (FR-R4) — keep the launcher's cache in lock-step
      // with the DB write so the next activation() short-circuits
      // without an app reload. If the SettingsPanel is rendered in
      // isolation (tests) there's no shared launcher — the local one
      // still gets the cached flag via the same call.
      (externalLauncher ?? launcher).setRecentsEnabled(next);
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<div class="settings" role="dialog" aria-label={STRINGS.SETTINGS_ARIA} data-testid="settings-panel">
  <header>
    <h2>{STRINGS.SETTINGS_TITLE}</h2>
    <button type="button" class="close" onclick={onClose} aria-label={STRINGS.CLOSE_ARIA}>×</button>
  </header>

  <section>
    <label for="hotkey-input">{STRINGS.HOTKEY_SECTION}</label>
    <div class="row">
      <input
        bind:this={hotkeyInputEl}
        id="hotkey-input"
        type="text"
        placeholder={STRINGS.HOTKEY_PLACEHOLDER}
        data-testid="hotkey-input"
      />
      <button type="button" class="primary" onclick={saveHotkey} data-testid="hotkey-save"
        >{STRINGS.SAVE_BTN}</button
      >
    </div>
  </section>

  <section>
    <span>{STRINGS.THEME_SECTION}</span>
    <div class="row">
      {#each ["light", "dark", "system"] as t (t)}
        <label>
          <input
            type="radio"
            name="theme"
            value={t}
            checked={theme === t}
            onchange={() => pickTheme(t as Theme)}
            data-testid={`theme-${t}`}
          />
          {t}
        </label>
      {/each}
    </div>
  </section>

  <section>
    <label>
      <input
        type="checkbox"
        checked={autostart}
        onchange={toggleAutostart}
        data-testid="autostart-toggle"
      />
      {STRINGS.LAUNCH_AT_LOGIN}
    </label>
  </section>

  <section>
    <label>
      <input
        type="checkbox"
        checked={recentsEnabled}
        onchange={toggleRecents}
        data-testid="recents-toggle"
      />
      {STRINGS.SAVE_RECENTS}
    </label>
  </section>

  <section>
    <span>{STRINGS.DIAGNOSTICS_SECTION}</span>
    <div class="row">
      <button
        type="button"
        onclick={() => (externalLauncher ?? launcher).copyDiagnostics()}
        data-testid="copy-diagnostics"
      >
        {STRINGS.COPY_DIAGNOSTICS_BTN}
      </button>
    </div>
  </section>

  {#if status}
    <div
      class="status"
      class:err={status.kind === "err"}
      role="status"
      data-testid="settings-status"
    >
      {status.msg}
    </div>
  {/if}
</div>

<style>
  .settings {
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
  section > label,
  section > span {
    display: block;
    margin-bottom: 4px;
    color: var(--muted, #888);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .row label {
    text-transform: none;
    letter-spacing: 0;
    color: inherit;
    font-size: 13px;
    margin-bottom: 0;
  }
  input[type="text"] {
    flex: 1;
    background: var(--input-bg, rgba(255, 255, 255, 0.06));
    border: 1px solid var(--border, rgba(255, 255, 255, 0.12));
    color: inherit;
    padding: 6px 10px;
    font: inherit;
    border-radius: 4px;
  }
  button.primary {
    background: var(--accent, #4a9eff);
    color: white;
    border: none;
    padding: 6px 12px;
    border-radius: 4px;
    cursor: pointer;
    font: inherit;
  }
  .status {
    margin-top: 12px;
    padding: 6px 10px;
    border-radius: 4px;
    background: var(--status-bg, rgba(74, 158, 255, 0.15));
    font-size: 12px;
  }
  .status.err {
    background: var(--status-err-bg, rgba(255, 80, 80, 0.18));
  }
</style>
