Execute the documentation pass described in `.agents/rust-documentation.md`.

Your job is to apply that plan to the Rust code under `src-tauri/` exactly as written. Treat the plan file as the source of truth for scope, workflow, quality bar, anti-laziness guardrails, and completion criteria.

Requirements:

1. Read `.agents/rust-documentation.md` first and follow it literally.
2. Inventory every `.rs` file under `src-tauri/` before making edits, record the starting file count, and maintain a reviewed-file checklist.
3. Review every file in scope, including production code, binaries, `build.rs`, unit tests, integration tests, nested test modules, and shared test helpers.
4. Do not skip files because they look small, obvious, repetitive, or unimportant.
5. Add or improve documentation only where needed. Preserve existing good docs and replace stale or misleading docs.
6. Do not change executable behavior unless absolutely required to make documentation truthful, and if that happens, call it out explicitly.
7. For every meaningful public API, add useful `///` documentation.
8. For complex private helpers, add documentation or targeted inline comments where needed.
9. For every long, multi-phase, stateful, concurrency-heavy, cleanup-heavy, or algorithmically dense function, use the required layered approach from the plan:
   - Layer 1: top-level `///` docstring
   - Layer 2: phase breakdown in working notes / terminal output before editing
   - Layer 3: selective inline comments at meaningful intermediate boundaries inside the function body
10. Do not be lazy with long functions. A top-level docstring alone is not sufficient.
11. Prefer comments that explain why, invariants, ownership, lifecycle, edge cases, cleanup contracts, and non-obvious decisions. Do not narrate obvious syntax.
12. Keep the docs high-signal and readable. Do not produce bloated or repetitive documentation.

Execution workflow:

1. Read `.agents/rust-documentation.md`.
2. Build the full Rust file inventory under `src-tauri/`.
3. Group the inventory by crate/subsystem or file category.
4. Review each file and classify it as:
   - no changes needed
   - documentation added/improved
   - blocked pending clarification/verification
5. Identify all long functions during the sweep and maintain a running list.
6. Apply documentation updates across the full inventory.
7. Run the strongest practical verification from the plan.
8. Before finishing, cross-check the final reviewed set against the original inventory so nothing was skipped.

Final output requirements:

1. Summarize what documentation was added or improved.
2. Report the Rust file inventory count.
3. Confirm that the full inventory was reviewed.
4. Report the long-function set, or at minimum the count, and confirm each received the required layered treatment.
5. Report verification results honestly, including anything not run.
6. Explicitly call out any skipped files, blockers, or incomplete areas. Silent omissions are not allowed.

Begin by reading `.agents/rust-documentation.md` and building the full `src-tauri/**/*.rs` inventory.
