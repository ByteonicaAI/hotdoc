---
title: Packs
description: What a pack is, what's in one, and the full list of packs hotdoc ships with.
---

## What's a pack

A pack is a single JSON file, curated by hand, living under `packs/curate/` in the hotdoc
repository. Each pack covers one tool — `git`, `docker`, `ssh`, and so on — as a collection of
command cards.

hotdoc ships with **18 curated packs, 729 command cards in total**.

## Card fields

Every command card in a pack has:

- `title` — the command's short name
- `syntax` — the exact command syntax
- `description` — what the command does
- `examples` (optional) — one or more `{ description, code }` pairs showing the command in use
- `tags` — keywords used for search
- `aliases` — short intent phrases that help fuzzy search find the card (e.g. "undo last commit")
- `source` — where the card came from: `official`, `cheat-sheet`, `curated`, or `personal`
- `source_url` (optional) — a link to the source, openable from a result with `Ctrl+Enter` (see
  [Daily use](../daily-use/))

## The packs

| Pack       | ID          |
| ---------- | ----------- |
| aws-cli    | `aws-cli`   |
| bash       | `bash`      |
| curl       | `curl`      |
| Docker     | `docker`    |
| GitHub CLI | `gh`        |
| Git        | `git`       |
| jq         | `jq`        |
| kubectl    | `kubectl`   |
| make       | `make`      |
| nginx      | `nginx`     |
| pnpm       | `pnpm`      |
| psql       | `psql`      |
| python     | `python`    |
| ripgrep    | `ripgrep`   |
| ssh        | `ssh`       |
| systemctl  | `systemctl` |
| terraform  | `terraform` |
| tmux       | `tmux`      |

You can jump straight to any one of these from the launcher with the command palette — type `>`
followed by the pack ID, e.g. `> git`. See [Daily use](../daily-use/) for details.
