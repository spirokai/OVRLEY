# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`.agents/agents.md`** — project overview, glossary, code style, state management patterns, build commands. This is the canonical source for the project's domain language.
- **`docs/CONTEXT.md`** — detailed architecture guide covering frontend component tree, Rust backend pipeline, data flow, widget system, and key architectural decisions.
- **`docs/adr/`** — if any exist, read ADRs that touch the area you're about to work in. (Currently none exist.)

If any of these files don't exist, **proceed silently**. Don't flag their absence; don't suggest creating them upfront.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in `.agents/agents.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal — either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/grill-with-docs`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding.
