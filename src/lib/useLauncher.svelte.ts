import { writeText as clipboardWrite } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { hideWindow, searchPacks } from "./tauri";
import type { SearchHit } from "./types";

// ponytail: Svelte 5 hooks live in `.svelte.ts` files. The runes `$state` and
// `$derived` only track reactively when fields are read directly off `this`.
// In the template, every handler is wrapped as `(e) => launcher.onFoo(e)` so
// `this` binds to the instance — otherwise the event listener calls the
// method with the DOM element as `this` and all reads go through `undefined`.

const SEARCH_DEBOUNCE_MS = 30;
const TOAST_MS = 1000;
const HIDE_AFTER_COPY_MS = 300;

export class Launcher {
  query = $state("");
  results = $state<SearchHit[]>([]);
  toast = $state<string | null>(null);

  zeroResult = $derived(this.query.trim() !== "" && this.results.length === 0);

  #toastTimer: ReturnType<typeof setTimeout> | null = null;
  #hideTimer: ReturnType<typeof setTimeout> | null = null;
  #debounce: ReturnType<typeof setTimeout> | null = null;

  #showToast(msg: string) {
    this.toast = msg;
    if (this.#toastTimer) clearTimeout(this.#toastTimer);
    this.#toastTimer = setTimeout(() => (this.toast = null), TOAST_MS);
  }

  #clearTimers() {
    if (this.#toastTimer) {
      clearTimeout(this.#toastTimer);
      this.#toastTimer = null;
    }
    if (this.#hideTimer) {
      clearTimeout(this.#hideTimer);
      this.#hideTimer = null;
    }
  }

  reset() {
    this.#clearTimers();
    this.query = "";
    this.results = [];
    this.toast = null;
  }

  async doHide() {
    this.#clearTimers();
    await hideWindow();
  }

  async runSearch() {
    const q = this.query.trim();
    if (!q) {
      this.results = [];
      return;
    }
    this.results = await searchPacks(q);
  }

  onInput(value: string) {
    this.query = value;
    if (this.#debounce) clearTimeout(this.#debounce);
    this.#debounce = setTimeout(() => void this.runSearch(), SEARCH_DEBOUNCE_MS);
  }

  async #copyAndToast(text: string) {
    await clipboardWrite(text);
    this.#showToast(`Copied: ${text}`);
  }

  async activate(shift: boolean, ctrl: boolean) {
    const top = this.results[0];
    if (!top) return;
    if (ctrl && top.source_url) {
      await openUrl(top.source_url);
      await this.doHide();
      return;
    }
    const text = shift ? (top.example_code ?? top.syntax) : top.syntax;
    await this.#copyAndToast(text);
    this.#hideTimer = setTimeout(() => void this.doHide(), HIDE_AFTER_COPY_MS);
  }

  onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      void this.doHide();
      return;
    }
    if (e.key === "Enter" && this.results[0]) {
      e.preventDefault();
      void this.activate(e.shiftKey, e.ctrlKey || e.metaKey);
      return;
    }
    if (e.key === "c" && (e.ctrlKey || e.metaKey) && this.query === "") {
      e.preventDefault();
      void this.doHide();
    }
  }
}
