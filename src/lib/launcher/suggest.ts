// ponytail: spec §7.6 — zero-result fallback. When a non-empty query
// matches nothing, suggest up to 3 closest pack ids by edit distance to
// the first query token (e.g. "dcoker comp" → "docker"). Pure function.

import { queryTokens } from "./highlight";

/** Levenshtein edit distance (iterative, two-row). */
export function editDistance(a: string, b: string): number {
  if (a === b) return 0;
  if (a.length === 0) return b.length;
  if (b.length === 0) return a.length;
  let prev = Array.from({ length: b.length + 1 }, (_, i) => i);
  let curr = new Array<number>(b.length + 1).fill(0);
  for (let i = 1; i <= a.length; i++) {
    curr[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      curr[j] = Math.min(curr[j - 1]! + 1, prev[j]! + 1, prev[j - 1]! + cost);
    }
    [prev, curr] = [curr, prev];
  }
  return prev[b.length]!;
}

/**
 * Up to `max` pack ids closest to the query's first token. A pack only
 * qualifies if its distance is within ceil(len/2)+1 of the token, so a
 * near-miss typo surfaces its pack but pure noise yields nothing. A
 * substring/prefix relationship counts as distance 0 (e.g. "docker logs"
 * → "docker"). Ties broken by shorter pack id, then lexicographically.
 */
export function suggestPacks(query: string, packIds: Iterable<string>, max = 3): string[] {
  const tokens = queryTokens(query);
  const tok = tokens[0];
  if (!tok) return [];
  // Tight cutoff: only near-misses qualify (a substring/prefix match is
  // distance 0 and always passes). floor(len/3) keeps "dcoker"→"docker"
  // (d=2) while rejecting unrelated packs for a long token.
  const cutoff = Math.max(1, Math.floor(tok.length / 3));
  const scored: Array<{ id: string; d: number }> = [];
  for (const id of packIds) {
    if (typeof id !== "string" || !id) continue;
    const low = id.toLowerCase();
    const d = low.includes(tok) || tok.includes(low) ? 0 : editDistance(tok, low);
    if (d <= cutoff) scored.push({ id, d });
  }
  scored.sort((x, y) => x.d - y.d || x.id.length - y.id.length || (x.id < y.id ? -1 : 1));
  return scored.slice(0, max).map((s) => s.id);
}
