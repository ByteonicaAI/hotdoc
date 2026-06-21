import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import App from "./App.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "@tauri-apps/api/core";
import type { InvokeArgs } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";

const mockHit = {
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
      return Promise.resolve(undefined);
    });
    vi.mocked(writeText).mockClear();
    vi.mocked(openUrl).mockClear();
  });

  it("renders the search input and focuses it", () => {
    render(App);
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
    expect(input).toBeInTheDocument();
  });

  it("typing debounces and fires search after ~30ms", async () => {
    vi.mocked(invoke).mockResolvedValue([mockHit]);
    render(App);
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
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
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
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
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
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
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
    await waitFor(() => {
      expect(openUrl).toHaveBeenCalledWith("https://git-scm.com");
    });
  });

  it("Ctrl+Enter rejects non-https source_url (SEC-3)", async () => {
    const hostile = { ...mockHit, source_url: "javascript:alert(1)" };
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search" ? [hostile] : undefined),
    );
    render(App);
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
    await waitFor(() => {
      expect(openUrl).not.toHaveBeenCalled();
      expect(screen.getByText(/only https: URLs allowed/i)).toBeInTheDocument();
    });
  });

  it("Escape hides the window", async () => {
    render(App);
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
    await fireEvent.keyDown(input, { key: "Escape" });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("hide_window");
    });
  });

  it("Ctrl+C with empty input hides the window", async () => {
    render(App);
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
    await fireEvent.keyDown(input, { key: "c", ctrlKey: true });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("hide_window");
    });
  });

  it("zero-result state renders the No-matches row", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    render(App);
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
    await fireEvent.input(input, { target: { value: "asdfqwer" } });
    await waitFor(() => {
      expect(screen.getByText(/no matches for "asdfqwer"/i)).toBeInTheDocument();
    });
  });

  it("Enter triggers record_recent with the trimmed query", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search" ? [mockHit] : undefined),
    );
    render(App);
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
    await fireEvent.input(input, { target: { value: "git stash" } });
    await waitFor(() => expect(screen.getByText("git stash")).toBeInTheDocument());
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("record_recent", { query: "git stash" });
    });
  });

  it("Escape does NOT trigger record_recent", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === "search" ? [mockHit] : undefined),
    );
    render(App);
    const input = screen.getByPlaceholderText(/hotdoc: type to search/i);
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
    const button = screen.getByRole("button", { name: /Recent query: git stash/i });
    await fireEvent.click(button);
    const inputEl = screen.getByPlaceholderText(/hotdoc: type to search/i);
    await waitFor(() => {
      expect((inputEl as HTMLInputElement).value).toBe("git stash");
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("search", { query: "git stash" });
    });
  });
});
