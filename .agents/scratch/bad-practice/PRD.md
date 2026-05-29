Status: draft

# Bad Practice Remediation

## Problem Statement

The current `app/` codebase has several recurring architecture and state-management patterns that increase maintenance cost and bug risk without delivering a better user experience. The highest-signal problems are:

1. Browser storage is read and sometimes mutated during module import, which makes initialization impure and tightly couples parts of the app to a browser environment.
2. Config synchronization depends on a module-global timer flag, which makes correctness depend on timing rather than explicit state transitions.
3. Overlay selection is owned by both local hook state and the global store, then synchronized with effects and refs.
4. Several hooks mirror props or store state into local state and repair drift with `useEffect`, creating extra sources of truth.
5. App-close persistence behavior is scattered across hooks and store slices instead of being removed as a clear product rule.
6. `JSON.stringify` is used as a catch-all tool for deep clone, equality, and effect dependencies, which obscures intent and adds avoidable work.

## Goal

Create a set of independently grabbable implementation issues that deepen the relevant modules while enforcing a new product rule: closing the app must discard all in-app state unless the user explicitly saved a template as a `.json` file.

The resulting work should preserve in-session editing behavior, preview behavior, selection behavior, render behavior, and template save/load workflows. The only intentional behavior change is that app-close persistence disappears; reopening the app should start from clean in-memory defaults unless the user loads a saved template file.

## Non-Goals

- No visual redesign
- No intentional workflow changes beyond removing app-close persistence
- No feature removals
- No template format breaking changes
- No broad migration away from Zustand or React hooks

## User Experience Constraints

- Existing `.json` templates must continue to load successfully.
- Closing and reopening the app must not restore settings, editor state, timeline state, debug flags, or unsaved template changes from browser storage.
- Editor behavior, render behavior, selection behavior, and preview behavior must remain functionally equivalent from a user perspective.
- The only durable source of user data is an explicitly saved template `.json`.
- Any refactor should prefer smaller, explicit seams over new framework abstractions that hide behavior.

## Issue Set

1. Remove browser storage hydration and import-time side effects
2. Replace timer-based config synchronization guard with explicit update flow
3. Unify overlay selection ownership
4. Reduce prop/store mirroring into local state
5. Remove app-close persistence and enforce template-only durability
6. Replace stringify-driven change detection and cloning in hot paths
7. Remove UI and network side effects from store slices
8. Split video preview and playback orchestration into deeper modules

## Risk and Difficulty

Risk means likelihood of user-visible regressions during the refactor. Difficulty means implementation complexity and coordination cost.

| Issue | Risk | Difficulty | Notes |
|---|---|---|---|
| `01` Remove browser storage hydration and import-time side effects | Medium | Medium | Broad touch surface, but conceptually straightforward. Main risk is startup/default-state regressions. |
| `02` Replace timer-based config synchronization guard with explicit update flow | High | High | Correctness-sensitive state coordination. Easy to introduce subtle config/timeline regressions. |
| `03` Unify overlay selection ownership | High | High | Highly interactive behavior with many edge cases: multi-select, delete, reorder, and drag flows. |
| `04` Reduce prop/store mirroring into local state | Medium | Medium | Usually manageable, but draft-vs-derived behavior can regress if the wrong local state is removed. |
| `05` Remove app-close persistence and enforce template-only durability | Medium | Medium | Product rule is simple, but there are many read/write paths to remove consistently. |
| `06` Replace stringify-driven change detection and cloning in hot paths | Medium-High | Medium-High | Some changes are mechanical, but dirty-state and remeasurement behavior are subtle. |
| `07` Remove UI and network side effects from store slices | Medium | Medium-High | Strong architectural payoff, but orchestration seams and error-flow locations will move. |
| `08` Split video preview and playback orchestration into deeper modules | High | High | Timing-heavy behavior with several interacting state machines and high regression potential. |

## Recommended Order

Recommended implementation order:

1. `01` Remove browser storage hydration and import-time side effects
2. `05` Remove app-close persistence and enforce template-only durability
3. `04` Reduce prop/store mirroring into local state
4. `07` Remove UI and network side effects from store slices
5. `06` Replace stringify-driven change detection and cloning in hot paths
6. `02` Replace timer-based config synchronization guard with explicit update flow
7. `03` Unify overlay selection ownership
8. `08` Split video preview and playback orchestration into deeper modules

Why this order:

- `01` and `05` establish the new persistence model early.
- `04` and `07` simplify state ownership and orchestration seams before deeper behavioral refactors.
- `06` removes hidden coupling in change detection before more invasive state-coordination work.
- `02`, `03`, and `08` are the most behavior-sensitive and benefit from stronger tests plus cleaner seams first.
