import { describe, it, expect, vi, beforeEach } from "vitest";
import { setupLauncher, type LifecycleRefs } from "./lifecycle";
import type { Launcher } from "../useLauncher.svelte";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.4.0"),
}));
vi.mock("../tauri", () => ({
  getAllSettings: vi.fn().mockResolvedValue({ theme: "system", recents_enabled: "true" }),
  indexStatus: vi.fn().mockResolvedValue({ entry_count: 731, pack_count: 18 }),
}));

function fakeLauncher() {
  return {
    reset: vi.fn(),
    loadEmptyView: vi.fn(),
    openSettings: vi.fn(),
    reloadIndex: vi.fn(),
    copyDiagnostics: vi.fn(),
    applyTheme: vi.fn(),
    setRecentsEnabled: vi.fn(),
    initPalette: vi.fn(),
    detailsHit: null,
    doHide: vi.fn(),
    closeDetails: vi.fn(),
  } as unknown as Launcher;
}

describe("setupLauncher", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.innerHTML = '<input id="q" />';
  });

  it("focuses the search input on mount", () => {
    const launcher = fakeLauncher();
    const refs: LifecycleRefs = { status: { current: null }, appVersion: { current: "0.4.0" } };
    const q = document.getElementById("q") as HTMLInputElement;
    const focusSpy = vi.spyOn(q, "focus");
    setupLauncher(launcher, refs);
    expect(focusSpy).toHaveBeenCalledOnce();
  });

  it("registers a global keydown listener for Escape", () => {
    const launcher = fakeLauncher();
    const refs: LifecycleRefs = { status: { current: null }, appVersion: { current: "0.4.0" } };
    const addSpy = vi.spyOn(window, "addEventListener");
    setupLauncher(launcher, refs);
    expect(addSpy).toHaveBeenCalledWith("keydown", expect.any(Function), true);
  });

  it("returns cleanup that removes the keydown listener", () => {
    const launcher = fakeLauncher();
    const refs: LifecycleRefs = { status: { current: null }, appVersion: { current: "0.4.0" } };
    const removeSpy = vi.spyOn(window, "removeEventListener");
    const { cleanup } = setupLauncher(launcher, refs);
    cleanup();
    expect(removeSpy).toHaveBeenCalledWith("keydown", expect.any(Function), true);
  });

  it("applies theme and recents_enabled from boot settings", async () => {
    const launcher = fakeLauncher();
    const refs: LifecycleRefs = { status: { current: null }, appVersion: { current: "0.4.0" } };
    setupLauncher(launcher, refs);
    await new Promise((r) => setTimeout(r, 0));
    // eslint-disable-next-line @typescript-eslint/unbound-method
    expect(launcher.applyTheme).toHaveBeenCalledWith("system");
    // eslint-disable-next-line @typescript-eslint/unbound-method
    expect(launcher.setRecentsEnabled).toHaveBeenCalledWith(true);
  });

  it("populates status.current after indexStatus resolves", async () => {
    const launcher = fakeLauncher();
    const refs: LifecycleRefs = { status: { current: null }, appVersion: { current: "0.4.0" } };
    setupLauncher(launcher, refs);
    await new Promise((r) => setTimeout(r, 0));
    expect(refs.status.current).toEqual({ entry_count: 731, pack_count: 18 });
  });

  it("loads empty view + palette on mount", () => {
    const launcher = fakeLauncher();
    const refs: LifecycleRefs = { status: { current: null }, appVersion: { current: "0.4.0" } };
    setupLauncher(launcher, refs);
    // eslint-disable-next-line @typescript-eslint/unbound-method
    expect(launcher.loadEmptyView).toHaveBeenCalledOnce();
    // eslint-disable-next-line @typescript-eslint/unbound-method
    expect(launcher.initPalette).toHaveBeenCalledOnce();
  });
});
