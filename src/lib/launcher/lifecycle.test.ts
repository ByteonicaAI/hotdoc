import { describe, it, expect, vi, beforeEach } from "vitest";
import { setupLauncher, type LifecycleRefs } from "./lifecycle";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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

  it("unlistens late-resolving listeners that race cleanup (I6 fix)", async () => {
    // ponytail: I6 fix verification — if a listen() promise resolves AFTER
    // cleanup() has run, the unlisten fn must still fire so the IPC
    // listener doesn't leak as a ghost. Without the disposed flag, the
    // unlisten would be pushed to a discarded array.
    const flush = () => new Promise<void>((r) => setTimeout(r, 0));
    const refs: LifecycleRefs = { status: { current: null }, appVersion: { current: "0.4.0" } };

    // Phase 1: cleanup runs first, THEN a queued listen resolves.
    // The unlisten should fire from the disposed branch (not from the
    // normal drain, because cleanup's drain already ran with an empty list).
    let resolveLate!: (u: UnlistenFn) => void;
    const latePromise = new Promise<UnlistenFn>((r) => {
      resolveLate = r;
    });
    vi.mocked(listen).mockReturnValueOnce(latePromise);

    const { cleanup } = setupLauncher(fakeLauncher(), refs);
    cleanup();

    let lateUnlistenCalls = 0;
    resolveLate(() => {
      lateUnlistenCalls += 1;
    });
    await flush();
    expect(lateUnlistenCalls).toBe(1);

    // Phase 2: setup runs, the listen promise resolves BEFORE cleanup,
    // then cleanup runs — drains the queued unlisten via the normal path.
    let resolveEarly!: (u: UnlistenFn) => void;
    const earlyPromise = new Promise<UnlistenFn>((r) => {
      resolveEarly = r;
    });
    vi.mocked(listen).mockReturnValueOnce(earlyPromise);

    const earlyResult = setupLauncher(fakeLauncher(), refs);
    let earlyUnlistenCalls = 0;
    resolveEarly(() => {
      earlyUnlistenCalls += 1;
    });
    await flush();
    expect(earlyUnlistenCalls).toBe(0);

    earlyResult.cleanup();
    expect(earlyUnlistenCalls).toBe(1);
  });
});
