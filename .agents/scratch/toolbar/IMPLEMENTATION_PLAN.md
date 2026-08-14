Status: ready-for-agent

# Left Toolbar and Pinnable Drawer Implementation Plan

## Objective

Replace the widget drawer's attached text tab with a full-height app-shell toolbar and convert the left drawer into one shared, pinnable tool drawer. Preserve current unpinned widget-catalog behavior, add durable pinned startup restoration, and make pinned mode reflow the scene canvas and timeline without changing the right control panel.

Do not run a production build. Do not add feature tests. Preserve unrelated worktree changes.

## Target Architecture

The toolbar and shared drawer must be implemented as a separate feature module under `app/src/features/`. Add only the presentation, lifecycle hook, constants, and normalization utility this feature needs. The app shell composes that feature into the horizontal layout and supplies Widgets content from `widget-drawer`.

The finished frontend should have four clear responsibilities:

1. The layout slice owns the canonical shared-drawer state machine.
2. A preference hook exported by the toolbar feature hydrates and persists the optional durable drawer preference; the app shell invokes it at its composition boundary.
3. The toolbar feature owns the toolbar and shared drawer presentation. `App.jsx` owns their placement, the startup gate, and the horizontal allocation within the application shell.
4. The widget-drawer feature supplies Widgets content and its successful-add behavior without owning generic shell positioning or pin state.

The main-content row has this logical shape:

```text
toolbar | pinned drawer allocation (0rem or 15rem) | scene canvas + timeline | right control panel
            overlay drawer occupies the same origin when unpinned and open
```

## Phase 1: Define Canonical Drawer State

### 1. Replace widget-specific layout state

- Replace the independent widget-drawer boolean with shared drawer fields that represent:
  - whether preference hydration has completed;
  - whether the shared drawer is visible;
  - whether the shared drawer is pinned;
  - the active tool identifier.
- Use one canonical tool identifier for Widgets. Do not add aliases for the former widget-drawer state.
- Keep state minimal. Do not store derived facts such as drawer width, overlay mode, backdrop visibility, or transition class names.
- Make pinned-without-visible and pinned-without-active-tool states unreachable through store actions.

### 2. Add behavior-oriented actions

- Add an initialization action that accepts canonical hydrated preference state and establishes the startup drawer state atomically.
- Add a tool-selection action with these rules:
  - closed and unpinned: select and open the requested tool;
  - open, unpinned, and same tool: close the drawer;
  - open, unpinned, and different tool: replace content and remain open;
  - pinned: select the requested tool and remain visible.
- Add an overlay-dismiss action that closes only an unpinned drawer.
- Add a pin action that pins and keeps the active drawer visible.
- Add an unpin action that changes to overlay mode and keeps the drawer visible.
- If one set-pin action produces a clearer invariant than separate pin/unpin actions, use it, but keep product behavior explicit at the call sites.

### 3. Update selector hooks and consumers

- Expose the canonical layout state and actions through the existing selector-hook boundary using shallow selection.
- Remove generic shell consumers of widget-specific toggle and open names.
- Ensure keyboard handling and drawer content use the same state-machine actions as pointer controls rather than reproducing conditions locally.

## Phase 2: Add Preference Hydration and Persistence

### 1. Define the preference contract

- Add one stable key in the existing OVRLEY settings store for the left drawer preference.
- Store only:
  - pinned boolean;
  - active tool identifier.
- Do not store visibility. Visibility is derived at startup: pinned means visible; unpinned means closed.
- Do not store width, transition state, backdrop state, or widget catalog state.

### 2. Normalize optional preference ingress once

- Add a small pure normalization utility at the preference boundary.
- Accept optional absence and return the canonical unpinned Widgets startup preference.
- Treat unreadable or malformed values exactly like absence, per the PRD: silently return the unpinned Widgets startup preference.
- Validate the active tool against the current allowlist. Do not pass unknown persisted identifiers into layout consumers.
- Do not add defensive revalidation in the store, toolbar, drawer shell, or content component after normalization.

### 3. Create a shell-owned lifecycle hook

- On mount, read and normalize the drawer preference, then initialize the layout slice once.
- Ignore late completion after unmount.
- Keep loading failures silent. Do not set the global error alert and do not log them.
- Expose only whether shell layout initialization is complete if the composition layer needs that fact directly; prefer reading canonical initialized state through the selector hook.
- Persist pin and active-tool changes after initialization.
- Avoid writing the initial hydrated state back immediately unless the user changes it.
- Keep save failures silent and retain current in-memory state.
- Serialize preference writes, or otherwise guarantee latest-write-wins ordering, so rapid pin or tool changes cannot leave an older preference on disk after a newer one.
- Since the store plugin autosaves, do not add an explicit save operation or roll back in-memory layout state after a failed write.

### 4. Integrate with shell composition

- Invoke the preference lifecycle from the shell composition boundary alongside other shell-owned hooks.
- Continue starting preference-independent app bootstrap work during the startup gate where practical.
- Do not add Tauri IPC or Rust code; use the existing frontend preference-store wrapper.

## Phase 3: Build Reusable Toolbar and Drawer Presentation

### 1. Add the toolbar

- Add a presentational vertical-toolbar component to the new toolbar feature module.
- Give the rail a fixed logical width of `3rem`, full main-content height, and non-shrinking layout behavior.
- Render one Widgets button using an existing Lucide icon selected according to current visual conventions.
- Give the button an accessible name, pressed/selected state, and `Alt+W` shortcut metadata.
- Route clicks to the canonical select-tool action.
- Keep tool registration simple for one item while allowing another item to be added without rewriting the drawer state model. Avoid a speculative plugin framework.

### 2. Add the shared drawer frame

- Add a presentational shared-drawer frame to the new toolbar feature module. It receives active content, pinned state, visibility, pin action, and dismissal action.
- Keep it fixed at `15rem` logical width and full main-content height.
- Place the pin control in the top-right corner while reserving a content region below or around it that does not obscure the widget catalog.
- Render only the active tool's content. Inactive tool content must be unmounted.
- Do not let the drawer frame import widget management or activity-domain state.

### 3. Implement overlay mode

- Position the visible unpinned drawer immediately to the toolbar's right and over the scene-canvas/timeline workspace.
- Cover the complete workspace height without the former top and bottom inset.
- Add a transparent workspace backdrop below the drawer and above canvas/timeline interactions.
- Keep the toolbar and drawer controls above and outside the backdrop's blocked area.
- Backdrop click invokes overlay dismissal.
- Preserve stacking below dialogs and other modal application overlays while remaining above canvas chrome, timeline, and normal popovers where current widget option behavior requires it.
- Retain or replace the existing overlay marker used by keyboard-overlay arbitration so unrelated app shortcuts remain blocked while the temporary drawer is open.

### 4. Implement pinned mode

- Allocate `15rem` in normal horizontal layout between toolbar and workspace.
- Keep the drawer visible with no backdrop.
- Leave the right control panel's fixed width and position unchanged.
- Ensure the center workspace keeps `min-width: 0`, clips app-level overflow, and allows its existing internal canvas/timeline sizing to recompute.
- Do not add a narrow-window pin restriction, auto-unpin behavior, or horizontal page scrolling.

## Phase 4: Adapt Widgets Content

### 1. Separate catalog content from shell behavior

- Refactor the existing widget drawer component into Widgets drawer content, retaining its widget manager and activity-availability responsibilities.
- Remove absolute positioning, translation, backdrop, attached text tab, and generic Escape ownership from the widget feature.
- Keep the existing widget grid and option-popover behavior intact except for the explicit pinned Escape rule below.
- Keep widget addition delegated through the existing widget manager.

### 2. Apply mode-dependent add dismissal

- After a widget is added successfully, invoke the generic overlay-dismiss action.
- Rely on the action's invariant: it closes an unpinned drawer and leaves a pinned drawer unchanged.
- Do not branch on pinned state inside the grid or duplicate the close rule in option-popover code.

### 3. Preserve the right-side editor

- Make no responsibility changes to the existing right-side Widgets editor tab.
- Do not move selected-widget editing controls into the left drawer.
- Do not alter widget catalog entries, availability markers, or display-option generation.

## Phase 5: Update Keyboard Behavior

### 1. Route `Alt+W` through tool selection

- Replace the widget-specific toggle call in shell keyboard handling with selection of the Widgets tool.
- Preserve current form-field, repeat, default-prevented, and overlay arbitration rules.
- In unpinned mode, `Alt+W` opens or closes Widgets according to the canonical action.
- In pinned mode, `Alt+W` selects Widgets and leaves the drawer visible.

### 2. Centralize Escape dismissal

- Register Escape behavior at the shared drawer shell or a shell-owned hook rather than in Widgets content.
- Listen for the existing drawer-close command only while the drawer is visible and unpinned.
- In pinned mode, do not register or run drawer Escape dismissal.
- Explicitly prevent temporary UI inside the drawer from handling Escape while pinned. For the current Widgets tool, pass the mode-dependent behavior to `WidgetOptionPopover` and prevent its Radix `PopoverContent` Escape dismissal through the primitive's Escape event API. Merely omitting the shared drawer listener is insufficient because the popover owns its own dismissal behavior.
- Ensure unpinned Escape closes the drawer once and does not compete with duplicate old listeners.

### 3. Keep overlay shortcut blocking truthful

- Update the shell and player keyboard-overlay checks to recognize the generic unpinned drawer backdrop rather than a widget-specific implementation detail.
- Do not classify a pinned drawer as a modal keyboard overlay; normal editor and player shortcuts should continue according to their existing workspace rules, except for the explicit pinned Escape decision above.

## Phase 6: Animate One Allocation Slot and Block Input

### 1. Use one transition source

- Add one relative, non-shrinking drawer-allocation slot immediately after the toolbar. Transition only its width between `0` and `15rem`.
- Keep the `15rem` drawer absolutely positioned at the allocation slot's left edge in both modes. At zero slot width it overlays the workspace; at `15rem` slot width the same drawer occupies the reserved layout space. Do not switch positioning models or animate the drawer separately.
- Prefer CSS transitions and transition events over timers. If a timer fallback is unavoidable, centralize the duration constant so layout and input blocking cannot drift.
- Do not animate application-level transforms that would interfere with the existing global app scale.

### 2. Block workspace pointer input during reflow

- Track only whether a pin/unpin layout transition is active; do not persist it.
- Start blocking before changing the allocation.
- Place a transparent interaction blocker over the scene-canvas and timeline workspace for the transition duration, or disable pointer events on their shared container.
- Keep the toolbar and drawer available, but disable repeated pin input until the current allocation transition completes.
- Clear blocking from the relevant transition-end event and handle interrupted/unmounted transitions safely.
- Keyboard focus does not need to be moved solely because pointer input is blocked.

### 3. Let existing measurement seams respond

- Preserve the workspace container identity and normal flex behavior so ResizeObserver- or DOM-measurement-based canvas and timeline hooks receive width changes.
- Do not manually calculate scene-canvas scale or timeline range in the toolbar feature.
- Do not force a user zoom reset when pinning. The existing fit and viewport behavior should respond to container geometry according to current ownership.

## Phase 7: Add the Startup Gate

### 1. Add full-shell startup presentation

- Before layout preference initialization completes, render a full-viewport OVRLEY startup view instead of the app header or editor shell.
- Display exactly `OVRLEY is starting...` with styling consistent with the existing loading presentation.
- Ensure the view works inside the existing viewport and global stylesheet assumptions without exposing partially initialized shell controls.

### 2. Swap atomically to the initialized shell

- Once preference hydration completes, render the normal app shell directly in its correct pinned or unpinned geometry.
- A valid pinned preference must first appear with toolbar, visible pinned drawer, reflowed workspace, and restored active tool.
- Missing, malformed, or failed preference reads must first appear with the toolbar and closed unpinned drawer.
- The initial restoration should not run the pin/unpin animation or temporary workspace blocker; those are for user-triggered mode changes after initialization.

## Phase 8: Cleanup and Compatibility

### 1. Remove superseded implementation

- Remove the old attached vertical Widgets text tab and translate-based collapsed drawer shell.
- Remove widget-specific backdrop identifiers and state names from generic app-shell consumers.
- Remove duplicate Escape listeners and any now-unused imports or selectors.
- Keep the widget-drawer public API focused on catalog content, with the new toolbar feature owning generic toolbar/drawer presentation.

### 2. Remove or update obsolete legacy tests

- Treat compatibility maintenance as required: existing store, app-shell keyboard, and widget-drawer tests directly reference the removed widget-specific state, actions, text tab, and backdrop identifier.
- Remove obsolete assertions or minimally update test setup/imports so the existing suite no longer encodes the superseded implementation.
- Do not replace those assertions with new toolbar, pinning, persistence, startup, Escape-mode, or reflow behavior assertions; automated feature coverage remains out of scope by product decision.

### 3. Preserve architectural rules

- Components remain presentational. State-machine behavior and preference lifecycle live in the store, hooks, and pure utilities.
- Normalize persisted input once at ingress. Consumers receive canonical values without repeated coercion.
- Use one naming scheme for toolbar tools and shared drawer state. Do not retain aliases for old widget-specific layout fields.
- Do not add fallback branches for hypothetical future tools. The current allowlist contains Widgets and can be deliberately extended later.
- Keep edits surgical and avoid unrelated app-shell refactoring.

### 4. Update stale comments and documentation

- Update JSDoc and module comments that describe the layout slice as a widget-drawer toggle.
- Update app-shell and widget-drawer public API comments to describe the new responsibility boundary.
- Do not expand general architecture documentation unless an edited comment would otherwise become false.

## Verification

No automated tests or feature-specific manual acceptance pass are required, per the PRD.

After implementation:

1. Run frontend lint with `pnpm lint` from the repository root.
2. If lint reports Prettier errors in touched frontend files, run the repository's frontend formatting command only with explicit awareness that it may touch broader files; otherwise apply surgical formatting fixes.
3. Do not run `pnpm build`, `pnpm tauri build`, or any build wrapper without explicit user permission.
4. Inspect the final diff and confirm unrelated worktree changes were not modified.

Existing tests require compatibility cleanup because they reference the superseded widget-specific layout API and drawer shell. Do not add new toolbar, drawer, persistence, startup, Escape-mode, or reflow behavior assertions, and do not expand verification beyond the lint and diff checks required by the PRD.

## Completion Criteria

- A `3rem` full-main-content-height toolbar replaces the old Widgets text tab.
- Widgets opens one shared `15rem` drawer immediately to the toolbar's right.
- Unpinned mode preserves current outside-click, Escape, add-widget, toolbar-toggle, and `Alt+W` dismissal behavior.
- Pinned mode remains visible, ignores drawer Escape behavior, stays open after widget addition, and reflows both scene canvas and timeline.
- Unpinning leaves the drawer open as an overlay.
- The right control panel remains fixed and unchanged.
- Pin/unpin reflow animates and blocks workspace pointer input during the transition.
- Pinned state and active tool persist through the existing Tauri settings store.
- Startup waits for preference hydration and displays `OVRLEY is starting...` until the shell can render in its final geometry.
- Missing, malformed, unreadable, and unwritable preference conditions follow the agreed silent fallback and session-state behavior.
- Layout state, persistence, shell presentation, and Widgets content have distinct ownership with no compatibility aliases.
- Frontend lint passes, no production build is run, and unrelated worktree changes remain untouched.
