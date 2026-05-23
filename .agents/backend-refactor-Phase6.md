# Phase 6 - Rust Documentation Pass: Reusable Execution Brief

## Purpose

Produce clear, durable, non-bloated documentation for the Rust code under `src-tauri/` so a future contributor can follow ownership, control flow, invariants, and failure modes directly from the source.

This document is a reusable instruction set for an LLM executor. It is intentionally file-agnostic. It does not name specific modules or functions because it must remain valid as the codebase evolves.

The executor's job is not to "add some docs where convenient." The executor must perform a full coverage pass over the Rust codebase and document every place that materially benefits from explanation.

## Primary Outcome

After this pass:

1. Every Rust source file in scope has been reviewed.
2. Every module has a clear top-of-file explanation when appropriate.
3. Every meaningful public API has useful `///` docs.
4. Every long or multi-phase function is understandable from both its top-level docstring and selective intermediate comments inside the body.
5. Test files explain what they cover and what regressions they guard.
6. The documentation is complete enough to guide a new contributor, but restrained enough to avoid noise.

## Scope

Review every Rust source file under `src-tauri/`, including:

1. library code
2. nested module files
3. `mod.rs` files
4. `lib.rs`, `main.rs`, and binary entry points
5. `build.rs`
6. test files under any `tests/` directory
7. unit-test modules that live under `src/`
8. shared test helpers
9. benchmark binaries if present

Do not assume that only production modules need documentation. Test and tooling code are in scope too.

## Non-Negotiable Rules

1. Review the full file inventory before editing anything.
2. Do not skip files because they seem small, obvious, old, or "just tests."
3. Do not stop after public APIs. Long private functions and complex private types are also in scope.
4. Do not change executable behavior unless the user explicitly asked for a code fix as part of the documentation pass.
5. Prefer documentation that explains why, constraints, invariants, ownership, lifecycle, and edge cases over documentation that merely paraphrases syntax.
6. Do not add boilerplate comments to every line, branch, loop, or assignment.
7. Do not produce essay-length docstrings where a tight high-signal explanation is enough.
8. If a function is long, multi-phase, or stateful, document its internal phases with selective inline comments at the right boundaries. This is required, not optional.
9. If a file already has good documentation, keep it and improve only what is missing, misleading, stale, or too shallow.
10. If documentation would be misleading because the code is unclear, read more code until the behavior is understood. Do not guess.

## Required Execution Workflow

The executor must follow this order.

### Step 1 - Build a Complete Rust File Inventory

Enumerate every `.rs` file under `src-tauri/` before editing.

The executor must work from this full inventory, not from memory and not from a partial grep. The pass is incomplete until every file on the inventory has been reviewed.

Record the total file count at the start of the pass. If new Rust files appear during the work, add them to the inventory before continuing.

The executor should maintain a working checklist grouped by category:

1. crate roots and entry points
2. regular source modules
3. tests and test helpers
4. benchmark or tooling binaries

### Step 2 - Review Each File Before Editing

For each file:

1. identify what role the file plays
2. identify whether it needs module-level docs
3. identify public items lacking meaningful docs
4. identify private items that are complex enough to need docs
5. identify long functions that need internal phase comments
6. identify stale or misleading comments that should be corrected or removed

Do not patch blindly. Read enough local context to understand ownership and control flow first.

Each file should end the review in one of three states:

1. no changes needed
2. documentation added or improved
3. blocked pending clarification or verification

Do not leave a file unclassified.

### Step 3 - Document the File at the Right Levels

Apply the rules in the sections below for:

1. module-level docs
2. public item docs
3. private item docs where complexity warrants them
4. long-function layered documentation
5. test documentation

### Step 4 - Verify Completeness

Before declaring the work finished, confirm:

1. every file from the inventory was reviewed
2. every long function identified during review received the required treatment
3. no file was skipped because it looked unimportant
4. the docs compile cleanly
5. every reviewed file has a final status recorded

## Documentation Standards

### A. Module-Level Documentation

Use top-of-file module docs when a file defines a module with real ownership or architectural meaning. In Rust this is usually `//!`.

Module docs should answer the following, compactly:

1. What this module owns.
2. What it does not own.
3. How it fits into nearby modules or layers.
4. The most important invariants or lifecycle assumptions.
5. Threading or performance characteristics if those matter here.

Good module docs are architectural maps, not prose padding.

#### Module Doc Template

```rust
//! Brief summary of the module's responsibility.
//!
//! Owns: the concepts, logic, or types this module is authoritative for.
//! Does not own: closely related responsibilities handled elsewhere.
//!
//! Key responsibilities:
//! - responsibility A
//! - responsibility B
//!
//! Important invariants:
//! - invariant A
//! - invariant B
//!
//! Threading/performance notes:
//! - only if relevant
```

#### When Module Docs Are Required

Add or improve module docs when any of the following are true:

1. the file is a crate root or module root
2. the file owns a meaningful subsystem
3. the file coordinates multiple helpers or stages
4. the file contains threading, async orchestration, process management, caching, rendering, encoding, parsing, or state transitions
5. the file is a test suite with nontrivial fixture setup or coverage boundaries

#### When a Smaller Top Comment Is Enough

For tiny entry points or `build.rs`, a short top-of-file comment is acceptable if full module docs would be excessive. It must still explain the file's role.

### B. Public API Documentation

Every meaningful public item must be reviewed for docs. This includes:

1. `pub fn`
2. `pub struct`
3. `pub enum`
4. `pub trait`
5. `pub type`
6. `pub const` if its purpose is not obvious
7. `pub(crate)` items when they are important cross-module surfaces or central helpers

Do not mechanically document trivial getters or self-evident constants if the surrounding type docs already make them obvious. Everything else should be documented.

#### What Public Docs Must Explain

A useful public docstring should cover the parts that matter for safe and correct use:

1. purpose
2. when or why the caller uses it
3. important arguments or fields
4. return value meaning
5. error cases if applicable
6. panic behavior if applicable
7. performance characteristics if relevant
8. threading, ownership, or lifecycle assumptions if relevant

#### Public Function Template

```rust
/// Short summary of the function's role.
///
/// Explain the reason this function exists and the larger operation it supports.
/// Include any important preconditions or assumptions that are not obvious from
/// the signature alone.
///
/// # Arguments
///
/// * `arg` - what it represents, and any important constraints
///
/// # Returns
///
/// What the caller receives and how to interpret it.
///
/// # Errors
///
/// List the meaningful failure modes and when they happen.
///
/// # Panics
///
/// State whether it can panic, and under what invariant violation if so.
///
/// # Performance
///
/// Include only when it matters.
pub fn example(...) -> Result<T, E> { ... }
```

#### Public Type Template

```rust
/// Short summary of the type's purpose.
///
/// Explain what role this type plays, who constructs it, and what invariants its
/// fields or states are expected to maintain.
pub struct Example { ... }
```

### C. Private Documentation Requirements

Private items do not all need `///`, but they must not be ignored.

Add docs or comments for private items when they are:

1. central to a subsystem's behavior
2. easy to misuse
3. stateful or lifecycle-heavy
4. algorithmically tricky
5. concurrency-sensitive
6. doing non-obvious data normalization, interpolation, caching, rendering math, process control, or cleanup

Use the lightest tool that makes the code understandable:

1. module docs for context
2. `///` on a private helper when it behaves like an internal API
3. `//` comments at a tricky block when the issue is local to a phase or branch

### D. Long Function Documentation: Required Layered Approach

This section is mandatory. It is the guardrail that prevents an LLM from skipping the hard parts.

Any function that matches one or more of the following must be treated as a long function:

1. roughly 80-100+ lines
2. multiple logical phases
3. complex state transitions
4. multiple resource ownership handoffs
5. concurrency or cancellation behavior
6. significant error recovery or cleanup logic
7. dense mathematical or transformation logic
8. orchestration across multiple helpers or subsystems

For every such function, apply all three layers below.

#### Layer 1 - Top-Level Function Docstring

Add a real `///` docstring that explains the function end-to-end.

It must describe, when relevant:

1. overall purpose
2. when it is called
3. major inputs
4. what it returns or produces
5. major error cases
6. performance significance
7. thread/process/task lifecycle
8. cancellation or shutdown behavior
9. important invariants

This docstring should let a reader understand the function before diving into the body.

#### Layer 2 - Phase Breakdown Before Editing

Before modifying the function body, the executor must write a short phase breakdown in working notes or terminal output.

This planning artifact is required for every long function. It should split the function into named phases such as:

1. validation
2. setup
3. preparation
4. resource acquisition
5. hot loop
6. finalization
7. cleanup
8. error handling

This phase plan is not required to live in the source file, but the executor must create it while working. Its purpose is to force deliberate coverage instead of shallow commenting.

The executor should maintain a running list of all long functions identified during the pass. The final report must confirm that each one received Layer 1 and Layer 3 treatment.

#### Layer 3 - Intermediate Inline Comments Inside the Function

After the phase breakdown, add selective inline comments inside the function body at the important boundaries.

These comments are required at:

1. major phase transitions
2. ownership handoffs
3. thread/task/process spawn sites
4. cleanup and shutdown branches
5. tricky error-recovery blocks
6. non-obvious math or coordinate/time conversions
7. cache or buffer reuse logic
8. branches whose purpose is not obvious from the condition
9. points where an invariant established earlier is being relied on

These comments must explain why the code is structured this way or what assumption holds at that point.

They must not merely narrate syntax.

#### Good and Bad Inline Comment Examples

Bad:

```rust
// Loop over frames
for frame in frames {
    // Check cancel flag
    if cancelled {
        break;
    }
}
```

Good:

```rust
// The loop checks cancellation only at frame boundaries so rendering work stays
// atomic at the frame level. Interrupting mid-frame would leave partially updated
// buffers and make cleanup harder to reason about.
for frame in frames {
    if cancelled {
        break;
    }
}
```

#### Long-Function Coverage Rule

If a file contains a long function and the executor adds only a top-level `///` docstring without intermediate phase comments, the documentation pass is incomplete.

## Comment Quality Rules

All added documentation must follow these rules:

1. Explain why, not just what.
2. Prefer constraints, invariants, ownership, and intent over restating syntax.
3. Keep comments close to the code they clarify.
4. Avoid repeating information that is already obvious from names and types.
5. Avoid filler phrases such as "This function is used to..." unless the sentence adds real context.
6. Avoid one-docstring-fits-all boilerplate copied across files.
7. Use concrete wording when describing state transitions, cleanup contracts, or failure behavior.
8. Remove or rewrite stale comments that no longer match the code.

## Test Documentation Standards

Tests are part of the codebase and must be documented intentionally.

### File-Level Test Docs

Each nontrivial test file should begin with a `//!` doc comment that explains:

1. what behavior area the file covers
2. whether it is unit, integration, snapshot, rendering, fixture, or tool-assisted coverage
3. what fixtures or helpers it relies on, if any
4. what kinds of regressions it is meant to catch

### Function-Level Test Docs

Add `///` docs to nontrivial tests when the setup, scenario, or regression value is not obvious from the test name alone.

Document:

1. scenario under test
2. important setup or fixtures
3. expected result
4. regression guarded

Do not add boilerplate to tiny obvious tests whose names already say enough.

### Shared Test Helpers

Shared test support files are also documentation targets. Explain:

1. what fixtures or helpers they provide
2. how other tests are expected to use them
3. any assumptions about environment, paths, sample assets, or tooling

## Entry Points, Tooling, and Build Scripts

Small files still need enough context to orient the reader.

For binaries, crate roots, and `build.rs`, document:

1. what the file is responsible for
2. what it delegates elsewhere
3. any setup, environment, or side-effect expectations

Do not force full architectural essays into tiny files. A crisp top comment is enough when the role is narrow.

## Anti-Laziness Guardrails

The executor must actively avoid these failure modes:

1. documenting only files that were already open
2. documenting only public items and ignoring private orchestration
3. adding top-of-file docs but skipping long-function bodies
4. documenting only "important" modules and skipping tests or helper code
5. writing generic docstrings that could fit any function
6. adding comments to easy code while avoiding the hardest functions
7. stopping after a subset of files because the pass feels repetitive

If the codebase contains many similar files, the executor must still inspect each one. Similarity is not permission to skip review.

## Recommended Editing Strategy

Use a consistent sweep so coverage stays reliable:

1. inventory all Rust files
2. group them by crate or subsystem
3. document module headers first
4. document public items next
5. identify long functions and handle them deliberately
6. document tests and helpers
7. run verification
8. do a final skipped-file check against the original inventory

## Verification Requirements

At minimum, run the strongest verification that is practical for a documentation-only pass.

Preferred verification:

```powershell
cargo fmt --check
cargo check --workspace --all-targets
cargo test --no-run
cargo doc --no-deps
```

If the workspace is too large or environment-constrained, explain what could not be run. Do not pretend verification happened when it did not.

## Completion Checklist

The pass is complete only if all of the following are true:

- [ ] Every `.rs` file under `src-tauri/` was inventoried and reviewed.
- [ ] The starting inventory count was recorded and updated if new Rust files appeared.
- [ ] Every module or entry-point file now has documentation appropriate to its role.
- [ ] Every meaningful public API has useful docs.
- [ ] Complex private items received documentation where needed.
- [ ] Every long or multi-phase function received both a strong top-level docstring and selective intermediate inline comments.
- [ ] Test files and shared test helpers were reviewed and documented where needed.
- [ ] Stale, misleading, or contradictory comments in touched files were corrected.
- [ ] The resulting docs are informative without becoming repetitive or bloated.
- [ ] Verification was run and reported honestly.

## Final Reporting Requirements

When the executor finishes, the final report must include:

1. a short summary of what kinds of documentation were added or improved
2. the Rust file inventory count and confirmation that the full inventory was reviewed
3. the set of long functions identified during the pass, or at minimum the count plus confirmation that each received the required layered treatment
4. confirmation that long functions were documented at intermediate phase boundaries
5. verification results and any limitations

If any file or verification step was intentionally left incomplete, that must be stated explicitly. Silent omissions are not allowed.
