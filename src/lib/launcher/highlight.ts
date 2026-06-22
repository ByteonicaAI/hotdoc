// ponytail: spec §7.3 — wrap matched query tokens in <mark> on the
// secondary (description) line. Pure function, no DOM. Returns segments
// the component renders with {#each}; the text is escaped by Svelte per
// segment, so this stays SEC-2 safe (no {@html}, no raw injection).

export type Segment = { text: string; mark: boolean };

// Same token separators as the scorer's query pipeline (spec §7.1):
// whitespace, "-", "_", "/", ".".
const TOKEN_SPLIT = /[\s\-_/.]+/;

/** Distinct, lowercased, non-empty query tokens (leading ">" stripped). */
export function queryTokens(query: string): string[] {
  const body = query.trim().replace(/^>+\s*/, "");
  const seen = new Set<string>();
  for (const t of body.toLowerCase().split(TOKEN_SPLIT)) {
    if (t.length > 0) seen.add(t);
  }
  return [...seen];
}

/**
 * Split `text` into alternating plain / marked segments. A character is
 * marked iff it falls inside a case-insensitive occurrence of any query
 * token. Overlapping/adjacent matches are merged. The concatenation of
 * all `segment.text` equals `text` exactly (no loss, no re-encoding).
 */
export function highlight(text: string, query: string): Segment[] {
  const tokens = queryTokens(query);
  if (tokens.length === 0 || text.length === 0) {
    return text.length ? [{ text, mark: false }] : [];
  }
  const lower = text.toLowerCase();
  // Mark coverage per character index.
  const covered = new Array<boolean>(text.length).fill(false);
  for (const tok of tokens) {
    let from = 0;
    for (;;) {
      const idx = lower.indexOf(tok, from);
      if (idx === -1) break;
      for (let i = idx; i < idx + tok.length; i++) covered[i] = true;
      from = idx + tok.length;
    }
  }
  // Coalesce contiguous runs of equal coverage into segments.
  const segments: Segment[] = [];
  let start = 0;
  for (let i = 1; i <= text.length; i++) {
    if (i === text.length || covered[i] !== covered[start]) {
      segments.push({ text: text.slice(start, i), mark: covered[start] === true });
      start = i;
    }
  }
  return segments;
}
