export type SearchHit = {
  id: string;
  pack_id: string;
  title: string;
  syntax: string;
  description: string;
  source: string;
  source_url: string | null;
  example_code: string | null;
  score: number;
};

export const SOURCE_LABEL: Record<string, string> = {
  official: "Official",
  "cheat-sheet": "Cheat Sheet",
  curated: "Curated",
};

export function sourceLabel(source: string): string {
  return SOURCE_LABEL[source] ?? source;
}
