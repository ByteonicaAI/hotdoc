// NFR-10: single extraction point for all user-facing English strings.
// All UI text must reference this map — no hard-coded English literals in templates.
// ponytail: a few toasts interpolate a count or an error message. We use
// `$1` / `$2` placeholders + the `format()` helper rather than 12
// individual .replace() call sites. The keys stay short and the templates
// stay readable; the helper is pure-string with no stringification rules
// (callers pass String(n) themselves).
export function format(template: string, ...args: string[]): string {
  return template.replace(/\$(\d+)/g, (_m, i) => args[Number(i) - 1] ?? "");
}

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
  // Empty-view aria-labels (NFR-10: static aria-label text must live in STRINGS)
  ARIA_RECENT_QUERY: "Recent query",
  ARIA_PINNED: "Pinned",
  ARIA_POPULAR: "Popular",
  // ResultItem actions
  CARD_ACTIONS_ARIA: "Card actions",
  COPY_EXAMPLE_BTN: "Copy example",
  COPY_ALL_BTN: "Copy all",
  PIN_BTN: "Pin",
  UNPIN_BTN: "Unpin",
  OPEN_SOURCE_BTN: "Open source",
  PIN_ARIA: "pinned",
  // ResultItem report action
  REPORT_CARD_BTN: "Report",
  // About panel
  ABOUT_ARIA: "About hotdoc",
  ABOUT_TITLE: "About hotdoc",
  ABOUT_LICENSE: "MIT / Apache-2.0",
  ABOUT_CONTENT_LICENSE: "Bundled content: CC-BY-4.0",
  // Details pane (FR-C6)
  DETAILS_ARIA: "Card details",
  DETAILS_TITLE: "Details",
  DETAILS_SOURCE_LABEL: "Source",
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
  // Toast templates (NFR-10; use format(STRINGS.X, String(n)))
  TOAST_RELOAD_OK: "Reloaded: $1 entries",
  TOAST_RELOAD_FAIL: "Reload failed: $1",
  TOAST_DIAG_OK: "Copied diagnostics to clipboard",
  TOAST_DIAG_FAIL: "Diagnostics failed: $1",
  TOAST_PIN: "Pinned: $1",
  TOAST_UNPIN: "Unpinned: $1",
  TOAST_OPEN_REJECTED: "Open rejected: only https: URLs allowed",
  TOAST_OPEN_FAIL: "Open failed: $1",
  TOAST_SEARCH_FAIL: "Search failed: $1",
  TOAST_COPY_OK: "Copied: $1",
  TOAST_COPY_FAIL: "Copy failed: $1",
  TOAST_HELP: "↑/↓ navigate · Enter copy · Shift+Enter example · Ctrl+P pin · Ctrl+Shift+? palette",
  // About panel labels (M4.5-T2 — moved out of AboutPanel.svelte)
  ABOUT_VERSION_LABEL: "Version",
  ABOUT_APP_LICENSE_LABEL: "App license",
  ABOUT_CONTENT_LICENSE_LABEL: "Content license",
  ABOUT_ATTRIBUTION_LABEL: "Attribution",
  ABOUT_TABLE_PACK: "Pack",
  ABOUT_TABLE_LICENSE: "License",
  ABOUT_TABLE_HOMEPAGE: "Homepage",
  // Details pane labels (M4.5-T2 — moved out of DetailsPane.svelte)
  DETAILS_COMMAND_LABEL: "Command",
  DETAILS_PACK_LABEL: "Pack",
  DETAILS_EXAMPLE_LABEL: "Example",
  // Footer status (M4.5-T2 — moved out of App.svelte)
  FOOTER_STATUS: "$1 commands · $2 packs",
} as const;
