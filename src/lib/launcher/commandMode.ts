// ponytail: spec §4.2.1. Closed enum of palette commands; unknown `>`
// commands strip the prefix and run as normal search. Pure function —
// no DOM, no IPC, fully unit-testable.

export type CommandResult =
  | { kind: "recents" }
  | { kind: "settings" }
  | { kind: "about" }
  | { kind: "help" }
  | { kind: "recents-clear" }
  | { kind: "pack-filter"; pack_id: string }
  | { kind: "fallthrough"; query: string };

export function parseCommand(raw: string, validPackIds: ReadonlySet<string>): CommandResult {
  const trimmed = raw.trim();
  if (!trimmed.startsWith(">")) {
    return { kind: "fallthrough", query: trimmed };
  }
  const body = trimmed.slice(1).trim();
  if (body === "") {
    return { kind: "fallthrough", query: "" };
  }
  if (body === "recents") return { kind: "recents" };
  if (body === "settings") return { kind: "settings" };
  if (body === "about") return { kind: "about" };
  if (body === "help") return { kind: "help" };
  if (body === "recents clear") return { kind: "recents-clear" };
  if (validPackIds.has(body)) return { kind: "pack-filter", pack_id: body };
  return { kind: "fallthrough", query: body };
}
