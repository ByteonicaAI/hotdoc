import { describe, it, expect } from "vitest";
import { editDistance, suggestPacks } from "./suggest";

const PACKS = ["docker", "git", "kubectl", "gh", "curl"];

describe("editDistance", () => {
  it("computes Levenshtein distance", () => {
    expect(editDistance("docker", "docker")).toBe(0);
    expect(editDistance("dcoker", "docker")).toBe(2); // transposition = 2 edits
    expect(editDistance("", "abc")).toBe(3);
  });
});

describe("suggestPacks", () => {
  it("surfaces the closest pack for a typo of the first token", () => {
    expect(suggestPacks("dcoker comp", PACKS)).toEqual(["docker"]);
  });

  it("treats a substring/prefix relation as a direct hit", () => {
    expect(suggestPacks("docker logs", PACKS)).toEqual(["docker"]);
    expect(suggestPacks("kubectl get pods", PACKS)).toEqual(["kubectl"]);
  });

  it("returns nothing for pure noise", () => {
    expect(suggestPacks("xyzzy plugh", PACKS)).toEqual([]);
  });

  it("returns nothing for an empty query", () => {
    expect(suggestPacks("   ", PACKS)).toEqual([]);
  });

  it("caps at max and orders by distance", () => {
    const out = suggestPacks("gut", PACKS, 3); // close to git and gh
    expect(out.length).toBeLessThanOrEqual(3);
    expect(out[0]).toBe("git");
  });
});
