<script lang="ts">
  import { onMount } from "svelte";
  import { prefersReducedMotion } from "svelte/motion";
  import SearchView from "./lib/SearchView.svelte";
  import CopiedView from "./lib/CopiedView.svelte";
  import Toast from "./lib/Toast.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import AboutPanel from "./lib/AboutPanel.svelte";
  import { Launcher } from "./lib/useLauncher.svelte";
  import { setupLauncher, type LifecycleRefs } from "./lib/launcher/lifecycle";
  import { setWindowSize } from "./lib/tauri";
  import { STRINGS } from "./lib/strings";

  const launcher = new Launcher();
  const refs = $state<LifecycleRefs>({
    status: { current: null },
    appVersion: { current: "" },
  });

  // Visual fade durations for the Enter-to-copy dismiss choreography. Must stay
  // ≤ the phase gaps in useLauncher.svelte.ts (SEARCH_FADE_MS / DISMISS_FADE_MS)
  // so each fade finishes before the phase advances.
  const SEARCH_FADE_MS = 160;
  const COPIED_FADE_MS = 200;
  function fadeMs(ms: number): number {
    return prefersReducedMotion.current ? 0 : ms;
  }

  // Show the results area (and footer) once there's a query, or when there are
  // recents/pinned to surface. Empty + no history = just the search bar; the
  // rest of the (transparent) window stays empty until the user types.
  const showResults = $derived(
    !launcher.emptyQuery || launcher.pinnedList.length + launcher.recentList.length > 0,
  );

  // ponytail: WS-F phase 1 fix — window-height sync runs as an inline
  // $effect (not from a Launcher method) so the effect lives in this
  // component's scope and never throws `effect_orphan`. Settings/About
  // force tall (820); everything else stays compact (660). The Rust
  // side (`set_window_size` IPC) handles the actual resize; main.tall +
  // SearchView's CSS height transition mask the snap visually.
  const WINDOW_HEIGHTS = { compact: 660, tall: 820 } as const;
  $effect(() => {
    const target =
      launcher.settingsOpen || launcher.aboutOpen ? WINDOW_HEIGHTS.tall : WINDOW_HEIGHTS.compact;
    void setWindowSize(target);
  });

  $effect(() => {
    const _ = launcher.selectedIndex;
    document.querySelector('[role="listbox"] li.active')?.scrollIntoView({ block: "nearest" });
  });

  // Footer key hints, split into discrete chips. Contextual to whether the
  // user is searching (full set) or on the cold/empty view (minimal set).
  const footerKeys = $derived(
    (launcher.emptyQuery || launcher.results.length === 0
      ? STRINGS.FOOTER_KEYS_EMPTY
      : STRINGS.FOOTER_KEYS_SEARCH
    )
      .split("·")
      .map((s) => s.trim())
      .filter(Boolean),
  );

  onMount(() => {
    return setupLauncher(launcher, refs).cleanup;
  });
</script>

<button
  type="button"
  class="backdrop"
  aria-label={STRINGS.DISMISS_ARIA}
  onclick={() => void launcher.doHide()}
></button>
<main class:tall={launcher.settingsOpen || launcher.aboutOpen}>
  {#if launcher.phase === "search"}
    <SearchView
      {launcher}
      status={refs.status.current}
      {showResults}
      {footerKeys}
      exitMs={fadeMs(SEARCH_FADE_MS)}
    />
  {/if}
  {#if launcher.phase === "copied"}
    <CopiedView
      message={launcher.toast}
      inDelay={fadeMs(SEARCH_FADE_MS)}
      duration={fadeMs(COPIED_FADE_MS)}
    />
  {/if}
  {#if launcher.toast && launcher.phase === "search"}
    <Toast message={launcher.toast} />
  {/if}
  {#if launcher.settingsOpen}
    <SettingsPanel onClose={() => launcher.closeSettings()} {launcher} />
  {/if}
  {#if launcher.aboutOpen}
    <AboutPanel
      version={refs.appVersion.current}
      packs={launcher.packMetas}
      onClose={() => launcher.closeAbout()}
      onToast={(m: string) => launcher.showToast(m)}
    />
  {/if}
</main>
