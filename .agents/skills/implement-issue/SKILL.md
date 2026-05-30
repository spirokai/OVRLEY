---
name: implement-issue
description: Execute a ready-for-agent implementation issue end-to-end from its markdown spec and parent PRD, regardless of whether the work lands in frontend, backend, or shared code. Use when the user references an issue file path, says "implement issue", "work on issue", "fix issue", or asks an agent to carry out a planned slice described in `.agents/scratch/.../issues/*.md`.
---

# Implement Issue

## Quick start

```text
User: implement .agents/scratch/widget-drawer/issues/01-drawer-skeleton.md

Agent: reads issue -> reads PRD -> inspects repo conventions -> implements -> verifies -> updates issue status
```

## Workflow

### 1. Gather context

1. Read the issue file at `.agents/scratch/[feature]/issues/[NN-name].md`.
2. Check `Status:`. If it is not `ready-for-agent`, report that the issue is not AFK-ready and stop.
3. Check `## Blocked by`. If it lists unresolved blockers, report them and stop.
4. Read the parent PRD from the `## Parent` path.
5. Read repo guidance before making stack assumptions:
   - `.agents/agents.md`, `AGENTS.md`, `CLAUDE.md`, or equivalent
   - issue-tracker docs under `.agents/agents/`
   - stack manifests and scripts such as `package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`, `Makefile`, or workspace config
6. Explore the touched code paths and identify the smallest set of files needed to satisfy the issue.

### 2. Frame the slice

Before editing:

- Map each acceptance criterion to code changes or verification steps.
- Use the PRD's `Implementation Decisions` for architecture, contracts, naming, and edge cases.
- Respect the PRD's `Out of Scope` section.
- Stop and report if the issue conflicts with the PRD, is materially underspecified, or would require work beyond the intended slice.

### 3. Implement

Follow the issue's acceptance criteria exactly. Prefer existing project patterns over inventing new ones.

Implementation order should follow dependency flow rather than a fixed framework:

1. Core data models, schemas, and business logic first
2. Integration points and APIs next
3. UI, handlers, and composition layers last

While implementing:

- Match the repo's language and style conventions instead of assuming a stack.
- Reuse existing helpers, components, modules, and tests when they fit.
- Keep edits focused on the issue's slice; avoid opportunistic refactors unless required to land the change safely.
- Preserve backward compatibility unless the issue or PRD explicitly changes behavior.
- If the repo has multiple stacks, modify only the layers needed by the acceptance criteria.

### 4. Verify

Choose the smallest credible verification for the files you touched by inspecting repo scripts and toolchain first.

Examples:

- JavaScript or TypeScript: package-manager scripts such as `lint`, `test`, `typecheck`, or targeted test commands
- Rust: `cargo test`, `cargo check`, `cargo clippy`, or crate-scoped commands
- Python: `pytest`, `ruff`, `mypy`, or project-specific tasks
- Polyglot repos: run the relevant checks for each changed area, starting with the fastest high-signal command

Verification rules:

- Prefer targeted checks over full builds when they provide equivalent confidence.
- If automated tests do not exist, perform the strongest available static checks and note any required manual verification.
- If a verification command fails because of your change, fix it before reporting.
- If verification cannot run because of missing tooling, environment limits, or unrelated existing failures, report the exact blocker clearly.

### 5. Update issue status

- If the issue is completed, change `Status: ready-for-agent` to `Status: ready-for-human`.
- If the issue remains blocked, incomplete, or unverifiable, leave the status unchanged and explain why.
- Do not modify the parent PRD or sibling issues unless the issue explicitly requires it.
- Do not delete issue files unless instructed.

### 6. Report

Summarize the outcome against the issue, not as a generic changelog:

- List each acceptance criterion and whether it was completed.
- Mention verification actually run and its result.
- Call out any deviations, assumptions, follow-ups, or residual risks.
- If you stopped early, explain the exact blocker so a human can unblock the issue quickly.
