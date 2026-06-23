// ponytail: shared URL helpers. Lives in src/lib/url.ts so any
// component can import the same isHttpsUrl check that the
// launcher / Rust IPC / pack validator use — three sites, one
// implementation. SEC-3 / FR-C3 say: only https: URLs may open.
export function isHttpsUrl(u: string): boolean {
  try {
    return new URL(u).protocol === "https:";
  } catch {
    return false;
  }
}
