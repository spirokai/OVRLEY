Status: ready-for-human

# 01 - Remove Browser Storage Hydration and Import-Time Side Effects

## Parent

`.agents/scratch/bad-practice/PRD.md`

## Why this issue exists

Several modules currently read from `localStorage` during initialization, and some helper code mutates storage while computing initial values. That means importing state modules is not a pure operation: simply loading the module can depend on a browser environment and can change persisted data.

This causes three kinds of friction:

- It makes the app harder to test in non-browser environments because module import can fail or behave differently before the app has even mounted.
- It makes startup behavior harder to reason about because some defaults are coming from browser storage even though app-close persistence is no longer desired.
- It blurs the seam between "read app state", "mutate app state", and "legacy cleanup", which makes future migrations riskier.

The intended user-facing behavior is now different: the app should start from clean in-memory defaults on every launch unless the user explicitly loads a saved template `.json`.

## What to build

Remove browser-storage hydration for app state and eliminate import-time storage side effects.

The slice should enforce the new durability rule:

- Store creation must no longer depend on reading app state from `localStorage`.
- Module import must not read from or write to browser storage as part of initialization.
- Default state should come from in-memory defaults and loaded template files only.
- If legacy browser-storage keys need cleanup, that cleanup must be deliberate, bounded, and must not become a new durability mechanism.

The final design does not need a new library. Simple deletion of browser-storage hydration paths is preferable if it improves locality and keeps behavior explicit.

## Affected files

- `app/src/store/useStore.js`
- `app/src/store/store-utils.js`
- `app/src/store/slices/createEditorSlice.js`
- `app/src/store/slices/createTemplateSlice.js`
- `app/src/features/app-shell/hooks/useEditorShellState.js`

## Suggested plan

1. Identify every app-state value currently sourced from browser storage during module import or state initialization.
2. Replace those reads with explicit in-memory defaults or template-load-driven initialization.
3. Remove any browser-storage writes that exist only to restore state after the app closes.
4. If needed, add a narrow one-time cleanup path for legacy keys that does not participate in runtime state hydration.
5. Verify that launch behavior now starts from clean defaults while template load behavior remains intact.
6. Add tests around startup behavior and module initialization without browser-storage hydration.

## Practical testing strategy

- Add characterization tests around app startup that assert clean in-memory defaults are used when no template is explicitly loaded.
- Add targeted tests that import or initialize the store in an environment without `window` or without usable `localStorage` and assert startup does not fail.
- Add a regression test that loads a template `.json` and verifies the loaded template still becomes the active config and related state.
- Add a regression test that pre-populates legacy browser-storage keys before startup and verifies they do not restore app state on launch.
- Keep one manual QA check for close-and-reopen behavior: change a few settings, close the app, reopen it, and confirm the app starts from defaults.

## Acceptance criteria

- [x] Importing the store modules and editor preference modules no longer reads from or writes to browser storage as a side effect of module evaluation
- [x] App startup no longer hydrates editor state, settings, timeline state, or template state from browser storage
- [x] In-memory defaults and explicit template loading are the only sources of initial app state
- [x] Cleanup or migration behavior, if still needed, is explicit and does not restore app state on launch
- [x] App startup remains safe when `window` or `localStorage` is unavailable
- [x] Tests cover launch behavior without browser-storage hydration and confirm template load behavior still works

## Blocked by

None - can start immediately
