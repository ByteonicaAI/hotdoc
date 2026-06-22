// NFR-10: single extraction point for all user-facing English strings.
// All UI text must reference this map — no hard-coded English literals in templates.
export const STRINGS = {
  // Search bar
  SEARCH_PLACEHOLDER: "hotdoc: type to search…",
  SEARCH_RESULTS_ARIA: "Search results",
  // Empty / zero-result states
  EMPTY_STATE: "Type to search packs.",
  NO_MATCHES_PREFIX: "No matches for “",
  NO_MATCHES_SUFFIX: "”",
  SUGGEST_PREFIX: "Did you mean:",
  INDEXING_STATUS: "Indexing…",
  // Empty-view section labels
  RECENT_LABEL: "Recent",
  POPULAR_LABEL: "Popular",
  // ResultItem actions
  CARD_ACTIONS_ARIA: "Card actions",
  COPY_EXAMPLE_BTN: "Copy example",
  COPY_ALL_BTN: "Copy all",
  PIN_BTN: "Pin",
  UNPIN_BTN: "Unpin",
  OPEN_SOURCE_BTN: "Open source",
  PIN_ARIA: "pinned",
  // Settings panel
  SETTINGS_ARIA: "Settings",
  SETTINGS_TITLE: "Settings",
  CLOSE_ARIA: "Close",
  HOTKEY_SECTION: "Hotkey",
  HOTKEY_PLACEHOLDER: "Ctrl+Shift+Space",
  SAVE_BTN: "Save",
  THEME_SECTION: "Theme",
  LAUNCH_AT_LOGIN: "Launch at login",
  SAVE_RECENTS: "Save recents on activation",
  DIAGNOSTICS_SECTION: "Diagnostics",
  COPY_DIAGNOSTICS_BTN: "Copy diagnostics",
} as const;
