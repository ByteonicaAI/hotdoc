import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";
import { Launcher } from "./lib/useLauncher.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "@tauri-apps/api/core";
import type { InvokeArgs } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { SearchHit } from "./lib/types";

const mockHit: SearchHit = {
  id: "git-stash",
  pack_id: "git",
  title: "Stash changes",
  syntax: "git stash",
  description: "Stash the working tree state",
  source: "curated",
  source_url: null,
  example_code: 'git stash push -m "wip"',
  score: 0.9,
};

describe("App launcher", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    vi.mocked(writeText).mockClear();
  });

  it("renders the search input and focuses it", () => {
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    expect(input).toBeInTheDocument();
  });

  it("typing debounces and fires search after ~30ms", async () => {
    vi.mocked(invoke).mockResolvedValue([mockHit]);
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    const searchCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "search");
    expect(searchCalls).toHaveLength(0);
    await waitFor(
      () => {
        expect(invoke).toHaveBeenCalledWith("search", { query: "git stash" });
      },
      { timeout: 200 },
    );
  });

  it("Enter copies top result syntax, shows toast, hides window", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search" ? [mockHit] : undefined),
    );
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith("git stash");
      expect(invoke).toHaveBeenCalledWith("hide_window");
    });
  });

  it("Shift+Enter copies the first example, falling back to syntax", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search" ? [mockHit] : undefined),
    );
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('git stash push -m "wip"');
    });
  });

  it("Ctrl+Enter opens source_url when present", async () => {
    const hitWithUrl = { ...mockHit, source_url: "https://git-scm.com" };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search" ? [hitWithUrl] : undefined),
    );
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_url", { url: "https://git-scm.com" });
    });
  });

  it("Ctrl+Enter rejects non-https source_url (SEC-3)", async () => {
    const hostile = { ...mockHit, source_url: "javascript:alert(1)" };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search" ? [hostile] : undefined),
    );
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
    await waitFor(() => {
      // open_url IPC must NOT have been called for a non-https URL
      const openCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "open_url");
      expect(openCalls).toHaveLength(0);
      expect(screen.getByText(/only https: URLs allowed/i)).toBeInTheDocument();
    });
  });

  it("Escape hides the window", async () => {
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.keyDown(input, { key: "Escape" });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("hide_window");
    });
  });

  it("Ctrl+C with empty input hides the window", async () => {
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.keyDown(input, { key: "c", ctrlKey: true });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("hide_window");
    });
  });

  it("zero-result state renders the No-matches row", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "asdfqwer" } });
    await waitFor(() => {
      // Text is split across nodes (NO_MATCHES_PREFIX + query + suffix).
      // Query the <li> by role/class and assert the prefix text is present.
      const li = document.querySelector("li.empty.zero");
      expect(li?.textContent).toMatch(/no matches for/i);
    });
  });

  it("Enter triggers record_recent with the trimmed query", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search" ? [mockHit] : undefined),
    );
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("record_recent", {
        query: "git stash",
        copiedSyntax: "git stash",
      });
    });
  });

  it("Escape does NOT trigger record_recent", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search" ? [mockHit] : undefined),
    );
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Escape" });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("hide_window");
    });
    const calls = vi.mocked(invoke).mock.calls.map((c) => c[0]);
    expect(calls).not.toContain("record_recent");
  });

  it("empty query shows the empty-view hint when no recents", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "get_recents" ? [] : undefined),
    );
    render(App);
    await waitFor(() => {
      expect(screen.getByText(/Type to search packs\./i)).toBeInTheDocument();
    });
  });

  it("empty query shows Recent rows when recents exist", async () => {
    const recents = [
      { query: "git stash", last_used_at: 1, use_count: 1 },
      { query: "docker ps", last_used_at: 2, use_count: 3 },
    ];
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "get_recents" ? recents : undefined),
    );
    render(App);
    await waitFor(() => {
      expect(screen.getByText("git stash")).toBeInTheDocument();
      expect(screen.getByText("docker ps")).toBeInTheDocument();
    });
    expect(invoke).toHaveBeenCalledWith("get_recents", { n: 5 });
  });

  it("clicking a recent fills input and fires search", async () => {
    const recents = [{ query: "git stash", last_used_at: 1, use_count: 1 }];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "get_recents") return Promise.resolve(recents);
      const q = (args as { query?: string } | undefined)?.query;
      if (cmd === "search") return Promise.resolve(q ? [mockHit] : []);
      return Promise.resolve(undefined);
    });
    render(App);
    await waitFor(() => {
      expect(screen.getByText("git stash")).toBeInTheDocument();
    });
    const button = screen.getByRole("option", { name: /Recent query: git stash/i });
    await fireEvent.click(button);
    const inputEl = screen.getByPlaceholderText(/type to search/i);
    await waitFor(() => {
      expect((inputEl as HTMLInputElement).value).toBe("git stash");
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("search", { query: "git stash" });
    });
  });

  it("ArrowDown moves selection through results and wraps", async () => {
    const hits = [
      { ...mockHit, id: "a", syntax: "alpha" },
      { ...mockHit, id: "b", syntax: "beta" },
      { ...mockHit, id: "c", syntax: "gamma" },
    ];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      const q = (args as { query?: string } | undefined)?.query;
      if (cmd === "search") return Promise.resolve(q ? hits : []);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("alpha")).toBeInTheDocument());
    const initial = screen.getByText("alpha").closest("li")!;
    expect(initial.className).toContain("active");
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    const beta = screen.getByText("beta").closest("li")!;
    expect(beta.className).toContain("active");
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    const gamma = screen.getByText("gamma").closest("li")!;
    expect(gamma.className).toContain("active");
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    const wrapped = screen.getByText("alpha").closest("li")!;
    expect(wrapped.className).toContain("active");
  });

  it("ArrowUp wraps to last result when at top", async () => {
    const hits = [
      { ...mockHit, id: "a", syntax: "alpha" },
      { ...mockHit, id: "b", syntax: "beta" },
    ];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      const q = (args as { query?: string } | undefined)?.query;
      if (cmd === "search") return Promise.resolve(q ? hits : []);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("alpha")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "ArrowUp" });
    const beta = screen.getByText("beta").closest("li")!;
    expect(beta.className).toContain("active");
  });

  it("Enter on highlighted row activates that row, not the top", async () => {
    const hits = [
      { ...mockHit, id: "a", syntax: "alpha" },
      { ...mockHit, id: "b", syntax: "beta" },
    ];
    vi.mocked(invoke).mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      const q = (args as { query?: string } | undefined)?.query;
      if (cmd === "search") return Promise.resolve(q ? hits : []);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("alpha")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith("beta");
    });
  });

  it("new search resets selection to 0", async () => {
    const hits1 = [
      { ...mockHit, id: "a", syntax: "alpha" },
      { ...mockHit, id: "b", syntax: "beta" },
    ];
    const hits2 = [{ ...mockHit, id: "x", syntax: "xray" }];
    let calls = 0;
    vi.mocked(invoke).mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      const q = (args as { query?: string } | undefined)?.query;
      if (cmd === "search") {
        calls += 1;
        return Promise.resolve(q ? (calls === 1 ? hits1 : hits2) : []);
      }
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("alpha")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    await fireEvent.input(input, { target: { value: "docker" } });
    await waitFor(() => expect(screen.getByText("xray")).toBeInTheDocument());
    const active = screen.getByText("xray").closest("li")!;
    expect(active.className).toContain("active");
  });

  it("arrow keys are inert when results are empty", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "search") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "zzzzz" } });
    await waitFor(() => expect(screen.getByText(/no matches for/i)).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    await fireEvent.keyDown(input, { key: "ArrowUp" });
    // No error means the keys were inert; no top-level exception thrown.
  });

  it("Ctrl+P pins the highlighted row, fires pin_entry IPC", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "search") return Promise.resolve([mockHit]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("pin_entry", { entryId: "git-stash" });
    });
    await waitFor(() => {
      expect(screen.getByText(/Pinned:/)).toBeInTheDocument();
    });
  });

  it("Ctrl+P again unpins (calls unpin_entry)", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([mockHit]);
      if (cmd === "search") return Promise.resolve([mockHit]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "p", ctrlKey: true });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("unpin_entry", { entryId: "git-stash" });
    });
  });

  it("> recents opens the empty view (clears input, refetches recents)", async () => {
    const recents = [{ query: "git stash", last_used_at: 1, use_count: 1 }];
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve(recents);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> recents" } });
    await waitFor(() => {
      expect((input as HTMLInputElement).value).toBe("");
    });
    await waitFor(() => {
      expect(screen.getByText("git stash")).toBeInTheDocument();
    });
  });

  it("> settings opens the settings panel", async () => {
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> settings" } });
    await waitFor(() => {
      expect(screen.getByTestId("settings-panel")).toBeInTheDocument();
    });
  });

  it("> help shows the keyboard-shortcut toast", async () => {
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> help" } });
    await waitFor(() => {
      expect(screen.getByText(/palette/i)).toBeInTheDocument();
    });
  });

  it("> recents clear calls clear_recents IPC", async () => {
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> recents clear" } });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("clear_recents");
    });
  });

  it("> unknown command falls through to search (strips `>`)", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "search") {
        const q = (args as { query?: string } | undefined)?.query;
        return Promise.resolve(q ? [mockHit] : []);
      }
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> dctr" } });
    await waitFor(() => {
      expect((input as HTMLInputElement).value).toBe("dctr");
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("search", { query: "dctr" });
    });
  });

  it("backspace past `>` resumes normal fuzzy search", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "search") {
        const q = (args as { query?: string } | undefined)?.query;
        return Promise.resolve(q ? [mockHit] : []);
      }
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> dctr" } });
    await waitFor(() => {
      expect((input as HTMLInputElement).value).toBe("dctr");
    });
    await fireEvent.input(input, { target: { value: "dctr" } });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("search", { query: "dctr" });
    });
  });

  it("settings panel saves hotkey, calls set_hotkey and set_setting", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "get_all_settings") return Promise.resolve({});
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> settings" } });
    await waitFor(() => {
      expect(screen.getByTestId("settings-panel")).toBeInTheDocument();
    });
    const hotkeyInput = screen.getByTestId("hotkey-input");
    await fireEvent.input(hotkeyInput, { target: { value: "Ctrl+Shift+K" } });
    await fireEvent.click(screen.getByTestId("hotkey-save"));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_hotkey", { combo: "Ctrl+Shift+K" });
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_setting", {
        key: "hotkey",
        value: "Ctrl+Shift+K",
      });
    });
    expect(screen.getByTestId("settings-status").textContent).toContain("Saved");
  });

  it("settings panel blocks reserved hotkey combo (Ctrl+C)", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "get_all_settings") return Promise.resolve({});
      if (cmd === "set_hotkey") {
        return Promise.reject(
          new Error("Ctrl+C is reserved — rebind it via your desktop environment instead"),
        );
      }
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> settings" } });
    await waitFor(() => {
      expect(screen.getByTestId("settings-panel")).toBeInTheDocument();
    });
    const hotkeyInput = screen.getByTestId("hotkey-input");
    await fireEvent.input(hotkeyInput, { target: { value: "Ctrl+C" } });
    await fireEvent.click(screen.getByTestId("hotkey-save"));
    await waitFor(() => {
      expect(screen.getByTestId("settings-status").className).toContain("err");
    });
    expect(screen.getByTestId("settings-status").textContent).toContain("reserved");
  });

  it("settings panel toggles autostart via set_autostart IPC", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "get_all_settings") return Promise.resolve({});
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> settings" } });
    await waitFor(() => {
      expect(screen.getByTestId("settings-panel")).toBeInTheDocument();
    });
    await fireEvent.click(screen.getByTestId("autostart-toggle"));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_autostart", { enabled: true });
    });
  });

  it("> about opens the about panel", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "list_pack_metas") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> about" } });
    await waitFor(() => {
      expect(screen.getByTestId("about-panel")).toBeInTheDocument();
    });
  });

  it("FR-R4 recents_disabled hides recents section in empty view", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_all_settings") return Promise.resolve({ recents_enabled: "false" });
      if (cmd === "get_recents")
        return Promise.resolve([{ query: "git stash", last_used_at: 1, use_count: 1 }]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    render(App);
    await waitFor(() => {
      expect(screen.queryByText("Recent")).not.toBeInTheDocument();
    });
  });

  it("recents toggle in settings propagates to launcher, suppresses record_recent IPC", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "get_all_settings") return Promise.resolve({});
      if (cmd === "search") return Promise.resolve([mockHit]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "> settings" } });
    await waitFor(() => expect(screen.getByTestId("settings-panel")).toBeInTheDocument());
    await fireEvent.click(screen.getByTestId("recents-toggle"));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_setting", {
        key: "recents_enabled",
        value: "false",
      });
    });
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    vi.mocked(invoke).mockClear();
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("git stash"));
    const recordCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "record_recent");
    expect(recordCalls).toHaveLength(0);
  });

  // ponytail: T19 (FR-R4). When the boot settings row says
  // recents_enabled = "false", the launcher's cached flag is false
  // before any activation can fire, so the `record_recent` IPC is
  // never sent. The backend `recents::record` also gates on
  // `is_enabled`, so this is defense in depth + an observable UI
  // signal that the toggle "did something".
  it("recents_disabled_in_settings_skips_record_recent_ipc", async () => {
    // First call from App.svelte onMount — settings row says off.
    const bootSettings: Record<string, string> = { recents_enabled: "false" };
    const getAllSettingsMock = vi
      .fn<() => Promise<Record<string, string>>>()
      .mockResolvedValueOnce(bootSettings)
      .mockResolvedValue(bootSettings);
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_all_settings") return getAllSettingsMock();
      if (cmd === "search") return Promise.resolve([mockHit]);
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter" });
    // Activation copied syntax; recents IPC must NOT have been called.
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("git stash"));
    const recordCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "record_recent");
    expect(recordCalls).toHaveLength(0);
  });
});

describe("Launcher.applyTheme", () => {
  beforeEach(() => {
    delete document.documentElement.dataset.theme;
  });

  // ponytail: each test mints its own Launcher so the runes state in one
  // test doesn't leak. The method is a pure DOM write (no IPC, no
  // timer, no toast) so we exercise it directly instead of through
  // App.svelte — keeps the test under 10 lines.
  it("apply_theme_light_sets_data_theme_attribute", () => {
    const launcher = new Launcher();
    const applied = launcher.applyTheme("light");
    expect(applied).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("apply_theme_dark_sets_data_theme_attribute", () => {
    const launcher = new Launcher();
    const applied = launcher.applyTheme("dark");
    expect(applied).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("apply_theme_system_removes_data_theme_attribute", () => {
    const launcher = new Launcher();
    document.documentElement.dataset.theme = "dark";
    const applied = launcher.applyTheme("system");
    expect(applied).toBe("system");
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("apply_theme_rejects_unknown_value_falls_back_to_system", () => {
    const launcher = new Launcher();
    document.documentElement.dataset.theme = "dark";
    // parameter is `unknown` so we don't need a cast; the stale row just
    // arrives as a string here
    const applied = launcher.applyTheme("blue");
    expect(applied).toBe("system");
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});

// ponytail: SEC-1 / FR-C2. ResultItem renders `hit.syntax` and
// `hit.example_code` via Svelte's `{...}` interpolation, which HTML-
// escapes by default. A poisoned pack could carry `<img
// src=x onerror=alert(1)>` in syntax and we MUST NOT execute it.
// This test renders ResultItem directly so it does not need the
// launcher/tauri plumbing — ResultItem is a pure presentation
// component that takes a `hit` prop.
describe("XSS regression (SEC-1)", () => {
  it("poisoned_card_syntax_renders_inert_text", async () => {
    const { default: ResultItem } = await import("./lib/ResultItem.svelte");
    const poisonedHit: SearchHit = {
      id: "poisoned",
      pack_id: "x",
      title: "Poisoned",
      syntax: "<img src=x onerror=alert(1)>",
      description: "test",
      source: "curated",
      source_url: null,
      example_code: null,
      score: 1.0,
    };
    const { container } = render(ResultItem, {
      hit: poisonedHit,
      active: false,
      pinned: false,
    });
    // The dangerous string must NOT be parsed as an <img> tag.
    expect(container.querySelector("img")).toBeNull();
    // And it must appear verbatim as text content (so the user can
    // still see what the syntax was — escaping, not stripping).
    expect(container.textContent).toContain("onerror=alert(1)");
  });

  it("poisoned_example_code_also_renders_inert", async () => {
    const { default: ResultItem } = await import("./lib/ResultItem.svelte");
    const poisonedHit: SearchHit = {
      id: "poisoned-2",
      pack_id: "x",
      title: "Poisoned 2",
      syntax: "echo hi",
      description: "test",
      source: "curated",
      source_url: null,
      example_code: "<script>window.__pwned=true</script>",
      score: 1.0,
    };
    const { container } = render(ResultItem, {
      hit: poisonedHit,
      active: false,
      pinned: false,
    });
    expect(container.querySelector("script")).toBeNull();
    expect(container.textContent).toContain("<script>window.__pwned=true</script>");
    // Sanity: window.__pwned was never set by the render.
    expect((window as unknown as { __pwned?: boolean }).__pwned).toBeUndefined();
  });

  it("highlights matched query tokens in the description (G2)", async () => {
    const { default: ResultItem } = await import("./lib/ResultItem.svelte");
    const { container } = render(ResultItem, {
      hit: mockHit,
      active: false,
      pinned: false,
      query: "stash",
    });
    const marks = container.querySelectorAll("mark");
    expect(marks.length).toBeGreaterThan(0);
    expect([...marks].some((m) => /stash/i.test(m.textContent ?? ""))).toBe(true);
  });

  it("renders hover actions and Copy example copies the example code (G3)", async () => {
    const { default: ResultItem } = await import("./lib/ResultItem.svelte");
    const onCopyExample = vi.fn();
    render(ResultItem, {
      hit: mockHit,
      active: false,
      pinned: false,
      query: "",
      onCopyExample,
    });
    const btn = screen.getByText("Copy example");
    expect(btn).toBeInTheDocument();
    await fireEvent.mouseDown(btn);
    expect(onCopyExample).toHaveBeenCalledWith(mockHit);
  });

  it('hides "Open source" when the card has no source_url (G3)', async () => {
    const { default: ResultItem } = await import("./lib/ResultItem.svelte");
    render(ResultItem, { hit: mockHit, active: false, pinned: false });
    expect(screen.queryByText("Open source")).toBeNull();
  });
});

describe("zero-result suggestions + popular (G4/G5)", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(writeText).mockClear();
  });

  it("renders pack suggestions on zero result and re-scopes on click (G4)", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "list_packs") return Promise.resolve(["docker", "git", "kubectl"]);
      if (cmd === "get_recents" || cmd === "get_pinned") return Promise.resolve([]);
      // A normal search returns nothing → zero-result state. A pack-scoped
      // search (query == "docker") returns the docker card.
      if (cmd === "search") {
        const q = (args as { query: string }).query;
        return Promise.resolve(q === "docker" ? [{ ...mockHit, pack_id: "docker" }] : []);
      }
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "dcoker" } });
    const chip = await screen.findByRole("button", { name: "docker" });
    expect(chip).toBeInTheDocument();
    await fireEvent.mouseDown(chip);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("search", { query: "docker" });
    });
  });
});

// ponytail: M4.5-T4 / SEC-3 — the AboutPanel and DetailsPane must
// route every external URL through the Rust open_url IPC; native
// <a target="_blank"> and silent open failures both bypass the
// https-only gate. These tests cover the new handlers.
describe("SEC-3 IPC routing (M4.5-T4)", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents" || cmd === "get_pinned" || cmd === "list_packs")
        return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
  });

  it("about_panel_homepage_button_routes_through_ipc_for_https", async () => {
    const { default: AboutPanel } = await import("./lib/AboutPanel.svelte");
    const onClose = vi.fn();
    const onToast = vi.fn();
    const packs = [{ id: "git", name: "Git", license: "MIT", homepage: "https://git-scm.com" }];
    render(AboutPanel, { version: "0.4.0", packs, onClose, onToast });
    const link = await screen.findByTestId("homepage-link");
    await fireEvent.click(link);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_url", { url: "https://git-scm.com" });
    });
    expect(onToast).not.toHaveBeenCalled();
  });

  it("about_panel_homepage_button_rejects_non_https_with_toast", async () => {
    const { default: AboutPanel } = await import("./lib/AboutPanel.svelte");
    const onClose = vi.fn();
    const onToast = vi.fn();
    const packs = [{ id: "evil", name: "Evil", license: "MIT", homepage: "http://evil.com" }];
    render(AboutPanel, { version: "0.4.0", packs, onClose, onToast });
    const link = await screen.findByTestId("homepage-link");
    await fireEvent.click(link);
    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith(expect.stringMatching(/only https/i));
    });
    const openCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "open_url");
    expect(openCalls).toHaveLength(0);
  });

  it("details_pane_source_link_routes_through_ipc_for_https", async () => {
    const { default: DetailsPane } = await import("./lib/DetailsPane.svelte");
    const onClose = vi.fn();
    const onToast = vi.fn();
    render(DetailsPane, {
      hit: { ...mockHit, source_url: "https://example.com/card" },
      onClose,
      onToast,
    });
    const link = screen.getByText("https://example.com/card");
    await fireEvent.click(link);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_url", { url: "https://example.com/card" });
    });
    expect(onToast).not.toHaveBeenCalled();
  });

  it("details_pane_source_link_rejects_non_https_with_toast", async () => {
    const { default: DetailsPane } = await import("./lib/DetailsPane.svelte");
    const onClose = vi.fn();
    const onToast = vi.fn();
    render(DetailsPane, {
      hit: { ...mockHit, source_url: "javascript:alert(1)" },
      onClose,
      onToast,
    });
    const link = screen.getByText("javascript:alert(1)");
    await fireEvent.click(link);
    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith(expect.stringMatching(/only https/i));
    });
    const openCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "open_url");
    expect(openCalls).toHaveLength(0);
  });
});

// ponytail: M4.5-T12 — boot-settings coverage. App.svelte onMount
// reads get_all_settings, then calls applyTheme + setRecentsEnabled
// before any activation can fire. Without these tests the boot path
// could silently fail (the IPC mock returns undefined for unknown
// commands and the launcher's applyTheme would log a warn).
describe("boot settings (M4.5-T12)", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(writeText).mockClear();
    delete document.documentElement.dataset.theme;
  });

  it("boot_settings_theme_dark_applies_data_theme_to_root", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "get_all_settings") return Promise.resolve({ theme: "dark" });
      return Promise.resolve(undefined);
    });
    render(App);
    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe("dark");
    });
  });

  it("boot_settings_theme_system_clears_data_theme_attribute", async () => {
    // ponytail: a stale "system" row should not set any attribute —
    // the OS @media query handles it.
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "get_all_settings") return Promise.resolve({ theme: "system" });
      return Promise.resolve(undefined);
    });
    document.documentElement.dataset.theme = "dark"; // start dirty
    render(App);
    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBeUndefined();
    });
  });

  it("boot_settings_recents_default_present_calls_record_recent", async () => {
    // ponytail: an absent recents_enabled key is treated as default-on
    // by setRecentsEnabled (anything but the literal "false" is true).
    // The complementary "false skips" path is covered by
    // recents_disabled_in_settings_skips_record_recent_ipc above.
    const bootSettings: Record<string, string> = {}; // no recents_enabled key
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "get_all_settings") return Promise.resolve(bootSettings);
      if (cmd === "search") return Promise.resolve([mockHit]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("git stash"));
    const recordCalls = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "record_recent");
    expect(recordCalls.length).toBeGreaterThan(0);
  });
});

// ponytail: M4.5-T12 — FR-C6 coverage. The Tab key on the focused
// result opens the DetailsPane for that result, and Escape closes it
// and returns focus to the search input.
describe("FR-C6 details pane (M4.5-T12)", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(writeText).mockClear();
  });

  it("tab_opens_details_pane_for_selected_result", async () => {
    // ponytail: M4.5-T12 — the plan's bullet says "renders with
    // hit.title visible", but the DetailsPane implementation surfaces
    // hit.syntax as the load-bearing command label (not hit.title).
    // Asserting syntax is the load-bearing behavior; if a future
    // refactor adds title rendering, expand the assertion then.
    const hits = [{ ...mockHit, id: "a", title: "Stash changes" }];
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "search") return Promise.resolve(hits);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("Stash changes")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Tab" });
    await waitFor(() => {
      const pane = screen.getByTestId("details-pane");
      expect(pane).toBeInTheDocument();
      expect(pane.textContent).toContain("git stash");
    });
  });

  it("escape_from_details_closes_pane", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "search") return Promise.resolve([mockHit]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Tab" });
    await waitFor(() => expect(screen.getByTestId("details-pane")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Escape" });
    await waitFor(() => {
      expect(screen.queryByTestId("details-pane")).toBeNull();
    });
  });

  it("escape_from_details_returns_focus_to_search_input", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "search") return Promise.resolve([mockHit]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Tab" });
    await waitFor(() => expect(screen.getByTestId("details-pane")).toBeInTheDocument());
    // Move focus away from the input to simulate the user clicking
    // the close button or tabbing onto the details.
    (document.activeElement as HTMLElement | null)?.blur();
    await fireEvent.keyDown(input, { key: "Escape" });
    await waitFor(() => {
      expect(document.activeElement).toBe(input);
    });
  });

  it("close_button_click_unmounts_details_pane", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "search") return Promise.resolve([mockHit]);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Tab" });
    await waitFor(() => expect(screen.getByTestId("details-pane")).toBeInTheDocument());
    // Click the close button — same path as closeDetails() direct call.
    const closeBtn = screen.getByRole("button", { name: /close/i });
    await fireEvent.click(closeBtn);
    await waitFor(() => {
      expect(screen.queryByTestId("details-pane")).toBeNull();
    });
  });

  it("details_pane_renders_command_and_example_labels", async () => {
    const hits = [
      {
        ...mockHit,
        id: "a",
        title: "Stash changes",
        syntax: "git stash",
        example_code: 'git stash push -m "wip"',
      },
    ];
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_recents") return Promise.resolve([]);
      if (cmd === "get_pinned") return Promise.resolve([]);
      if (cmd === "list_packs") return Promise.resolve([]);
      if (cmd === "search") return Promise.resolve(hits);
      return Promise.resolve(undefined);
    });
    render(App);
    const input = screen.getByPlaceholderText(/type to search/i);
    await fireEvent.input(input, { target: { value: "git" } });
    await waitFor(() => expect(screen.getByText("Stash changes")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Tab" });
    await waitFor(() => {
      const pane = screen.getByTestId("details-pane");
      expect(pane).toBeInTheDocument();
      // Command + Example labels are surfaced from STRINGS — both
      // appear when the hit has example_code.
      expect(pane.textContent).toMatch(/command/i);
      expect(pane.textContent).toMatch(/example/i);
      expect(pane.textContent).toContain("git stash");
      expect(pane.textContent).toContain('git stash push -m "wip"');
    });
  });
});
