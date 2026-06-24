// ponytail: must mirror Rust EntrySource (crates/hotdoc-core/src/pack.rs:9-15,
// serde rename_all = "lowercase"). Closed union prevents class injection via
// `class="source {hit.source}"` in ResultItem/DetailsPane.
export type SourceKind = "official" | "cheat-sheet" | "curated" | "personal";

export type SearchHit = {
  id: string;
  pack_id: string;
  title: string;
  syntax: string;
  description: string;
  source: SourceKind;
  source_url: string | null;
  example_code: string | null;
  score: number;
};

export type PackMeta = {
  id: string;
  name: string;
  license: string;
  homepage: string;
};

export type Recent = {
  query: string;
  copied_syntax: string | null;
  last_used_at: number;
  use_count: number;
};

export const SOURCE_LABEL: Record<SourceKind, string> = {
  official: "Official",
  "cheat-sheet": "Cheat Sheet",
  curated: "Curated",
  personal: "Personal",
};

export function sourceLabel(source: SourceKind): string {
  return SOURCE_LABEL[source];
}
