import { describe, it, expect } from "vitest";
import { highlight, queryTokens } from "./highlight";

describe("queryTokens", () => {
  it("splits on scorer separators, lowercases, dedups, strips leading >", () => {
    expect(queryTokens("> Docker-Compose logs")).toEqual(["docker", "compose", "logs"]);
    expect(queryTokens("git git GIT")).toEqual(["git"]);
    expect(queryTokens("   ")).toEqual([]);
  });
});

describe("highlight", () => {
  it("marks matched tokens, leaves the rest plain, and is lossless", () => {
    const segs = highlight("Show docker logs", "docker");
    expect(segs.map((s) => s.text).join("")).toBe("Show docker logs");
    const marked = segs.filter((s) => s.mark).map((s) => s.text);
    expect(marked).toEqual(["docker"]);
  });

  it("is case-insensitive and merges adjacent/overlapping matches", () => {
    const segs = highlight("DockerDocker", "docker");
    expect(segs).toEqual([{ text: "DockerDocker", mark: true }]);
  });

  it("returns a single plain segment when nothing matches", () => {
    expect(highlight("kubectl get pods", "docker")).toEqual([
      { text: "kubectl get pods", mark: false },
    ]);
  });

  it("does not treat injected markup specially — text is preserved verbatim, no mark", () => {
    // SEC-2: the helper never emits HTML; a <script> in card text stays as
    // inert text in a plain segment (the component escapes it on render).
    const evil = '<img src=x onerror="alert(1)">';
    const segs = highlight(evil, "docker");
    expect(segs).toEqual([{ text: evil, mark: false }]);
    expect(segs.some((s) => s.mark)).toBe(false);
  });
});
