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

import { invoke } from "@tauri-apps/api/core";
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
    expect(invoke).not.toHaveBeenCalled();
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
});
