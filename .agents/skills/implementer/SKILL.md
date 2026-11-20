---
name: implementer
description: Implement the task in a task file, within its scope, leaving the changes uncommitted for review.
disable-model-invocation: true
argument-hint: <task-file>
---

The task file is your whole scope: build what it asks, and nothing it doesn't.

## Inputs

- `task` — the file describing what to build.

## Process

1. **Split the task into requirements.** Read the task file in full, then write one line per distinct behaviour it asks for.
   Done when every sentence of the task maps to a requirement, or is context. No code is read in this step.

2. **Read what the requirements need, and nothing else.**
   - **Locate** with searches, not browsing: one `rg -n` per requirement for the identifiers, config keys or strings it names. Never list directories, open files to "get a feel", or read `docs/`.
   - **Read whole** (one call per file, never in line-range slices): every file you will edit, and the direct callers and callees of each function whose signature or behaviour you change: one hop, not further.
   - **Read by excerpt** (`rg -n -C 5 <name>`): types, helpers and conventions you use but don't change.
   - **Done** when you can name, for every requirement, the file and function you will change and each call site the change affects. Stop reading then and start building.
   - **Budget:** about 15 files before the first edit. If you hit it without reaching _done_, the task is underspecified there: stop reading, record the gap as an **assumption**, and build on the reading closest to the existing code.

   After this step, read a new file only when a compile error, failing test or edit points into it.

3. **Build** each requirement. Every edit follows `AGENTS.md`, its _Non-negotiable code rules_ first.
   Where the task is silent on a choice that changes behaviour, take the reading closest to the existing code and record it as an **assumption**.

4. **Check the touched code.**
   - Frontend files: `pnpm exec vp fmt <files> --write` and `pnpm exec vp lint <files>` from `app/`, plus the single test files that cover them (`pnpm exec vp test run <paths>`).
   - Rust: `cargo test` for the touched crate, narrowed to the relevant test names.

   Done when every check you ran is green, or its failure is recorded for the return.
