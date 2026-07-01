# Product

## Register

brand

## Users

Developers and sysadmins who already know a CLI tool's _name_ but blank on its exact flag syntax mid-task. They hit the hotdoc hotkey from inside another app, need the right invocation in seconds, and want to get back to what they were doing. The landing page's visitor is the same person pre-install: evaluating whether this is worth adding to their workflow, in the ~10 seconds before they bounce or click Download.

## Product Purpose

hotdoc is a local-first, keyboard-driven desktop launcher that answers "what's the exact syntax for the tool I already know?" in under five seconds — no AI lookup, no account, no network round-trip. The docs site's landing page exists to make that promise legible instantly: what it does, why it's trustworthy (local-only, no telemetry), and how to get it (download or read the guide).

## Brand Personality

Fast, plain, trustworthy. Not playful, not corporate-SaaS. The voice of a well-made CLI tool's man page: precise, unhurried about itself, no hype. Confidence comes from specificity (exact numbers: five seconds, 729 cards, 18 packs), not from adjectives.

## Anti-references

Not a SaaS-cream gradient-hero landing page. Not an AI-tool marketing page (no "supercharge your workflow," no purple-to-pink gradients, no glassmorphism). Not developer-tool-as-costume (mono type slathered everywhere just to signal "technical"). The product itself has no AI in it — the page shouldn't borrow AI-startup visual grammar.

## Design Principles

- Say the number, not the adjective — "under five seconds," "729 command cards," "18 packs" carry more trust than "blazing fast" or "huge library."
- The splash template does the work — this is a docs-site landing bolted onto Starlight, not a marketing site; restraint here means not fighting the theme's defaults with bespoke chrome.
- Local-first shows up in the copy, not just the pitch — network/account/telemetry absence is stated plainly, not framed as a privacy crusade.
- One hero, one grid, no scroll theater — a keyboard tool's landing page should load and resolve in one fold, same economy of motion the product itself promises.

## Accessibility & Inclusion

Inherits Starlight's baseline (semantic landmarks, keyboard-navigable nav, prefers-color-scheme support). No custom motion beyond what Starlight ships by default; if any is added, it must respect `prefers-reduced-motion`. Body/card copy must clear WCAG AA contrast against both light and dark Starlight themes.
