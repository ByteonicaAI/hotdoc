<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { writeText as clipboardWrite } from "@tauri-apps/plugin-clipboard-manager";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";

  type SearchHit = {
    id: string;
    pack_id: string;
    title: string;
    syntax: string;
    description: string;
    source: string;
    source_url: string | null;
    example_code: string | null;
    score: number;
  };

  const SOURCE_LABEL: Record<string, string> = {
    official: "Official",
    "cheat-sheet": "Cheat Sheet",
    curated: "Curated",
  };

  let query = $state("");
  let results = $state<SearchHit[]>([]);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;
  let hideTimer: ReturnType<typeof setTimeout> | null = null;
  const zeroResult = $derived(query.trim() !== "" && results.length === 0);

  function showToast(msg: string) {
    toast = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 1000);
  }

  async function doHide() {
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = null;
    }
    if (toastTimer) {
      clearTimeout(toastTimer);
      toastTimer = null;
    }
    await invoke("hide_window");
  }

  let debounce: ReturnType<typeof setTimeout> | null = null;
  function onInput(e: Event) {
    query = (e.currentTarget as HTMLInputElement).value;
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(() => void runSearch(), 30);
  }

  async function runSearch() {
    const q = query.trim();
    if (!q) {
      results = [];
      return;
    }
    results = await invoke<SearchHit[]>("search", { query: q });
  }

  async function activate(shift: boolean, ctrl: boolean) {
    const top = results[0];
    if (!top) return;
    if (ctrl && top.source_url) {
      await openUrl(top.source_url);
      await doHide();
      return;
    }
    if (shift) {
      const text = top.example_code ?? top.syntax;
      await clipboardWrite(text);
      showToast(`Copied: ${text}`);
    } else {
      await clipboardWrite(top.syntax);
      showToast(`Copied: ${top.syntax}`);
    }
    hideTimer = setTimeout(() => void doHide(), 300);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      void doHide();
    } else if (e.key === "Enter" && results[0]) {
      e.preventDefault();
      void activate(e.shiftKey, e.ctrlKey || e.metaKey);
    } else if (e.key === "c" && (e.ctrlKey || e.metaKey) && query === "") {
      e.preventDefault();
      void doHide();
    }
  }

  onMount(() => {
    document.getElementById("q")?.focus();
  });
</script>

<main>
  <input
    id="q"
    placeholder="hotdoc: type to search…"
    oninput={onInput}
    onkeydown={onKey}
    bind:value={query}
    autocomplete="off"
    autocorrect="off"
    spellcheck="false"
  />
  <ul role="listbox" aria-label="Search results">
    {#each results as r, i (r.id)}
      <li class:active={i === 0} role="option" aria-selected={i === 0}>
        <div class="row syntax-row">
          <span class="syntax">{r.syntax}</span>
          <span class="source {r.source}">{SOURCE_LABEL[r.source] ?? r.source}</span>
        </div>
        <div class="row meta-row">
          <span class="title">{r.title}</span>
          <span class="pack">{r.pack_id}</span>
        </div>
        <div class="desc">{r.description}</div>
        {#if r.example_code}
          <div class="example"><code>{r.example_code}</code></div>
        {/if}
      </li>
    {/each}
    {#if zeroResult}
      <li class="empty" aria-hidden="true">No matches for "{query.trim()}"</li>
    {/if}
  </ul>
  {#if toast}
    <div class="toast" role="status">{toast}</div>
  {/if}
</main>
