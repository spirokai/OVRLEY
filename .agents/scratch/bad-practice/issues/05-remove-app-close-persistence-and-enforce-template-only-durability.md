Status: ready-for-agent

# 05 - Remove App-Close Persistence and Enforce Template-Only Durability

## Parent

`.agents/scratch/bad-practice/PRD.md`

## Why this issue exists

App-close persistence behavior is currently spread across multiple hooks, store slices, and helper modules. Storage keys, serialization rules, and "should this be restored on next launch?" decisions are implemented close to individual call sites instead of as one explicit product rule.

That creates avoidable friction:

- removing browser-backed durability requires touching many modules
- storage key usage is harder to inventory and delete safely
- launch semantics become inconsistent because each module makes restoration decisions locally

The intended user-visible behavior is now explicit: no settings, template changes, editor timeline values, debug flags, or other app values should survive an app close. The only durable user data should be template `.json` files that the user explicitly saves and later loads.

## What to build

Remove app-close persistence for app state and make template `.json` files the only durability seam.

The resulting seam should make three rules explicit:

- runtime app state lives only in memory
- template save/load owns durable user data
- any legacy browser-storage cleanup is one-time and non-restorative

Call sites should no longer write app-close durable state to browser storage. If any browser storage remains temporarily during migration, it must not participate in restoring state after relaunch.

## Affected files

- `app/src/store/store-utils.js`
- `app/src/store/slices/createEditorSlice.js`
- `app/src/store/slices/createTemplateSlice.js`
- `app/src/store/slices/createMediaSlice.js`
- `app/src/store/useStore.js`
- `app/src/features/app-shell/hooks/useEditorShellState.js`
- `app/src/features/video-preview/hooks/useVideoPlaybackClock.js`
- `app/src/lib/previewPerf.js`

## Suggested plan

1. Inventory every value currently written to browser storage and classify whether it should become in-memory-only or be removed entirely.
2. Delete browser-storage writes for app-close durability across editor state, settings, template identity, timeline state, preview settings, and diagnostics flags.
3. Ensure template save/load continues to round-trip only through explicit `.json` files rather than implicit browser persistence.
4. Add a bounded cleanup path for legacy storage keys if needed, but do not read those keys to restore state.
5. Verify that relaunch always starts from clean defaults unless the user loads a template file.
6. Add tests covering launch/reset semantics and template-only durability.

## Practical testing strategy

- Add a small matrix of startup tests that pre-populate different legacy browser-storage keys and assert none of them restore state after launch.
- Add regression tests for explicit template save/load so `.json` remains the only durable path for config and template-related data.
- Add tests that verify runtime state still behaves normally within a session even though it is no longer written for relaunch.
- Add a launch-reset test that changes several values, simulates app restart, and asserts defaults are restored until a template is loaded.
- Keep a manual QA pass covering the exact product rule: edit state, close app, reopen app, confirm clean defaults, then load a saved template and confirm it restores only what is in the file.

## Acceptance criteria

- [ ] App-close persistence via browser storage is removed for the targeted app-state domains
- [ ] Closing and reopening the app starts from clean in-memory defaults rather than restoring prior unsaved state
- [ ] Template save/load through `.json` files remains intact and is the only durable user-data path
- [ ] Any legacy browser-storage cleanup is explicit and non-restorative
- [ ] Raw storage key usage is removed or reduced to migration-only behavior in the targeted modules
- [ ] Tests cover launch-reset semantics and template-only durability

## Blocked by

- [01 - Remove Browser Storage Hydration and Import-Time Side Effects](./01-remove-browser-storage-hydration-and-import-time-side-effects.md)
