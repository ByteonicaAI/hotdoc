import { describe, it, expect } from "vitest";
import { parseCommand } from "./commandMode";

const empty = new Set<string>();
const withDocker = new Set<string>(["docker"]);

describe("parseCommand", () => {
  it("non-`>` input falls through with original query", () => {
    expect(parseCommand("git stash", empty)).toEqual({
      kind: "fallthrough",
      query: "git stash",
    });
  });

  it("lone `>` falls through to empty query", () => {
    expect(parseCommand(">", empty)).toEqual({ kind: "fallthrough", query: "" });
  });

  it("`> recents` → recents", () => {
    expect(parseCommand("> recents", empty)).toEqual({ kind: "recents" });
  });

  it("`> settings` → settings", () => {
    expect(parseCommand("> settings", empty)).toEqual({ kind: "settings" });
  });

  it("`> help` → help", () => {
    expect(parseCommand("> help", empty)).toEqual({ kind: "help" });
  });

  it("`> recents clear` → recents-clear", () => {
    expect(parseCommand("> recents clear", empty)).toEqual({ kind: "recents-clear" });
  });

  it("`> <pack_id>` with valid id → pack-filter", () => {
    expect(parseCommand("> docker", withDocker)).toEqual({
      kind: "pack-filter",
      pack_id: "docker",
    });
  });

  it("`> <unknown>` falls through, strips `>`", () => {
    expect(parseCommand("> dctr", empty)).toEqual({
      kind: "fallthrough",
      query: "dctr",
    });
  });

  it("`> <pack_id> rest of query` is NOT pack-filter (only exact match)", () => {
    expect(parseCommand("> docker logs", withDocker)).toEqual({
      kind: "fallthrough",
      query: "docker logs",
    });
  });

  it("leading/trailing whitespace is trimmed before parse", () => {
    expect(parseCommand("  > recents  ", empty)).toEqual({ kind: "recents" });
  });

  it("validPackIds lookup is case-sensitive", () => {
    expect(parseCommand("> Docker", withDocker)).toEqual({
      kind: "fallthrough",
      query: "Docker",
    });
  });
});
