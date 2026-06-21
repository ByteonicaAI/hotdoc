<script lang="ts">
  import { getAllSettings, setAutostart, setHotkey, setSetting, type SettingsMap } from "./tauri";
  import { log } from "./logger";

  type Props = { onClose: () => void };
  const { onClose }: Props = $props();

  let hotkeyDraft = $state("");
  let theme = $state<"light" | "dark" | "system">("system");
  let autostart = $state(false);
  let recentsEnabled = $state(true);
  let status = $state<{ kind: "ok" | "err"; msg: string } | null>(null);

  $effect(() => {
    void (async () => {
      try {
        const s: SettingsMap = await getAllSettings();
        hotkeyDraft = s["hotkey"] ?? "Ctrl+Shift+Space";
        theme = (s["theme"] as "light" | "dark" | "system") ?? "system";
        autostart = s["autostart"] === "true";
        recentsEnabled = s["recents_enabled"] !== "false";
      } catch (e) {
        log.warn("load settings failed", { error: String(e) });
        hotkeyDraft = "Ctrl+Shift+Space";
      }
    })();
  });

  async function saveHotkey() {
    try {
      await setHotkey(hotkeyDraft);
      await setSetting("hotkey", hotkeyDraft);
      status = { kind: "ok", msg: "Saved." };
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function pickTheme(v: "light" | "dark" | "system") {
    theme = v;
    try {
      await setSetting("theme", v);
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }

  async function toggleAutostart() {
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
    const next = !recentsEnabled;
    try {
      await setSetting("recents_enabled", String(next));
      recentsEnabled = next;
    } catch (e) {
      status = { kind: "err", msg: String(e) };
    }
  }
</script>

<div class="settings" role="dialog" aria-label="Settings" data-testid="settings-panel">
  <header>
    <h2>Settings</h2>
    <button type="button" class="close" onclick={onClose} aria-label="Close">×</button>
  </header>

  <section>
    <label for="hotkey-input">Hotkey</label>
    <div class="row">
      <input
        id="hotkey-input"
        type="text"
        bind:value={hotkeyDraft}
        placeholder="Ctrl+Shift+Space"
        data-testid="hotkey-input"
      />
      <button type="button" class="primary" onclick={saveHotkey} data-testid="hotkey-save"
        >Save</button
      >
    </div>
  </section>

  <section>
    <span>Theme</span>
    <div class="row">
      {#each ["light", "dark", "system"] as t (t)}
        <label>
          <input
            type="radio"
            name="theme"
            value={t}
            checked={theme === t}
            onchange={() => pickTheme(t as "light" | "dark" | "system")}
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
      Launch at login
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
      Save recents on activation
    </label>
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
