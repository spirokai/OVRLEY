---
name: fixer
description: Apply the findings of a review file, recording one outcome per finding.
disable-model-invocation: true
argument-hint: <review-file> <task-file>
---

The review file is your whole scope: you change what its findings name, at the sites they name.

## Inputs

- `review file` — the round's review, ending in a verdict.
- `task` — the file describing what was to be built.

## Process

1. **Read** the review file and the task in full.

2. **Settle each finding**, in number order. Open its `path:line` first, then give it one outcome:
   - `fixed` — apply the suggested fix, or a smaller one that removes the same defect.
   - `rejected` — the finding is wrong; quote the code or task line that shows it.
   - `deferred` — a smell whose fix would widen the change beyond the task.

   Bugs end `fixed` or `rejected`. Every edit follows `AGENTS.md`.
   A fix includes its own cleanup: delete what your edit leaves unused (functions, imports, types, tests) and update the callers it breaks, wherever they are. Nothing else outside the named sites.
   Done when every finding number has exactly one outcome.

3. **Check the touched code.**
   - Frontend files: `pnpm exec vp fmt <files> --write` and `pnpm exec vp lint <files>` from `app/`, plus the single test files that cover them (`pnpm exec vp test run <paths>`).
   - Rust: `cargo test` for the touched crate, narrowed to the relevant test names.
   - Builds and the full suite are out of scope.

   Done when every check you ran is green, or its failure is recorded in the finding's entry.

4. **Append** to the end of the review file, leaving everything above untouched:

   ```markdown
   ## Fixes

   1. fixed — what changed (`path:line`)
   2. rejected — why, with the quoted line
   3. deferred — why it widens scope
   ```

5. **Return** the counts of fixed, rejected and deferred findings. Changes stay uncommitted.
