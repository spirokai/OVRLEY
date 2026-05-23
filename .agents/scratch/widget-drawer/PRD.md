Status: ready-for-agent

# Widget Drawer — Collapsible Quick-Add Palette

## Problem Statement

The OVRLEY overlay editor supports 10 widget types (Text, Speed, HR, Power, Cadence, Time, Temperature, Gradient, Elevation, Route Map), with plans to grow to ~30. Currently, the quick-add toolbar lives as a 5-column grid of icon-only buttons in the right sidebar's "Widgets" tab. As the widget count grows, this layout becomes unsustainable — the buttons are crammed into a narrow space, and the sidebar is already shared with settings, active widget management, and per-widget property editors. The growing palette needs a dedicated, expandable surface that doesn't compete for space with the editing workflow.

## Solution

Replace the quick-add toolbar in the sidebar with a collapsible drawer anchored to the left edge of the screen, below the app header. When collapsed it appears as a thin tab (bookmark) with a `Grid3X3` icon. When clicked, it expands into a vertical panel (`w-40`, 160px) spanning the full remaining height of the app. The tab animates with the panel — it's attached to the panel's edge, not a separate trigger.

The drawer holds a scrollable 2-column grid of square buttons (52×52px) each showing a widget icon with the full widget name in tiny text below. Buttons are not larger than the current sidebar quick-add buttons — the 52px dimension is approximately the same footprint. The drawer is non-modal: it does not block or blur the rest of the UI, and users can continue interacting with the canvas and sidebar while it's open.

Toggling the drawer is by clicking the tab or pressing Escape. Adding a widget does not close the drawer — users can add multiple widgets in sequence.

## User Stories

1. As an overlay editor user, I want to see a compact tab on the left edge of the screen, so that I know where the widget palette lives without it taking up space.
2. As an overlay editor user, I want to click the tab to expand it into a full-height panel, so that I can browse and select widgets.
3. As an overlay editor user, I want the tab to be visually attached to the panel (animating with it), so that the relationship between tab and panel is clear.
4. As an overlay editor user, I want the drawer to be non-modal, so that I can continue editing my overlay while it's open.
5. As an overlay editor user, I want to see widget buttons as square (52×52px) icons with labels underneath, so that I can quickly identify and add the widget I need.
6. As an overlay editor user, I want the buttons arranged in 2 columns, so that the drawer stays compact and I don't have to scroll much.
7. As an overlay editor user, I want the labels to use full widget names (e.g. "Heart Rate" not "HR"), so that I don't have to guess abbreviations.
8. As an overlay editor user, I want to click a widget button to add it to the scene, keeping the drawer open, so that I can add multiple widgets without re-opening.
9. As an overlay editor user, I want to close the drawer by clicking the tab again or pressing Escape, so that I can dismiss it when done.
10. As an overlay editor user, I want the drawer to start collapsed by default on app load, so that it doesn't intrude on my editing space until I need it.
11. As an overlay editor user, I want the drawer to be scrollable, so that it can accommodate the eventual ~30 widget types.
12. As the developer, I want the drawer state stored in a Zustand slice, so that keyboard shortcuts and future toolbar buttons can programmatically toggle it.

## Implementation Decisions

### Architecture

- **Custom implementation, not shadcn Drawer/Sheet.** Vaul (the underlying library for shadcn Drawer) is unmaintained, can't easily be positioned to start below the app header, and can't support the attached tab animation. Radix Sheet is modal by design and fights the non-modal requirement. A custom `<div>` with CSS `translateX` transition is simpler and gives full control.
- **No Framer Motion.** The codebase has no animation library, and adding one for a single `translateX` transition is unnecessary overhead. Plain CSS transitions suffice.
- **New `createLayoutSlice` in existing Zustand store.** A dedicated slice for UI chrome state, keeping it separate from domain state (`createEditorSlice`) to avoid unnecessary re-renders of canvas components. Contains a single boolean `widgetDrawerOpen` (default `false`) and a `toggleWidgetDrawer()` action.
- **Store access via `useShallow` selector.** Follows the existing pattern in `app/src/hooks/useAppStoreSelectors.js` to prevent re-render propagation.

### Modules to Build

1. **`app/src/store/slices/createLayoutSlice.js`** — New Zustand slice. State: `{ widgetDrawerOpen: false }`. Actions: `{ toggleWidgetDrawer() }`. Pattern matches existing slices (`createMediaSlice`, etc.) using Immer `set()`.

2. **`app/src/features/widget-editor/components/WidgetDrawer.jsx`** — The drawer component. Contains:
   - A fixed-position container inside the main content area, starting below the app header
   - A collapsed tab (~16px) with `Grid3X3` icon, gently rounded right corners, hover highlight
   - An expanded panel (`w-40`, 160px) with overflow-y-auto scrollable content
   - CSS transition on `translateX` for the entire tab+panel block
   - Uses the `QUICKMENU_ITEMS` data from `@/lib/widget-icons` for button definitions
   - Each button: `h-13 w-13` square, icon + label in tiny font below
   - 2-column grid layout (`grid grid-cols-2`) with gaps
   - Calls `addWidget(type)` from `useWidgetManager` on click

3. **`app/src/features/widget-editor/hooks/useWidgetDrawer.js`** — Hook that:
   - Reads `widgetDrawerOpen` and `toggleWidgetDrawer` from the store
   - Registers a `keydown` listener for Escape to close
   - Returns `{ isOpen, toggle, close }`

### Modules to Modify

4. **`app/src/store/useStore.js`** — Import and compose `createLayoutSlice` into the store state object alongside existing slices.

5. **`app/src/features/widget-editor/components/SidebarWidgetsTab.jsx`** — Remove the quick-add toolbar grid (currently lines 76-88). Keep the `Separator` and the "Active Widgets" accordion below it intact.

6. **`app/src/App.jsx`** — Add `<WidgetDrawer />` inside the main flex content area, as a sibling before the center panel + right sidebar. The drawer uses `absolute` positioning within the `relative` flex container to overlay the left edge of the main content.

### Layout Integration

The drawer mounts inside the existing `flex min-h-0 flex-1 overflow-hidden relative` container in `App.jsx`. The container becomes `relative` to anchor the drawer's `absolute` positioning. The drawer panel overlays the left edge of the center content (the `OverlayEditor`). The right sidebar is unaffected.

```
┌──────────────────────────────────────────┐
│ TitleBar                                 │
├──────────────────────────────────────────┤
│ AppHeader                                │
├────┬────────────────────────┬────────────┤
│    │                        │  Control   │
│ D  │   OverlayEditor        │  Panel     │
│ R  │   (canvas)             │  (w-96)    │
│ A  │                        │            │
│ W  │                        │            │
│ E  ├────────────────────────┤            │
│ R  │ OverlayPlayer          │            │
└────┴────────────────────────┴────────────┘
```

### Edge Cases

- **No widgets in config:** The drawer still renders buttons — the `addWidget` function handles creation. The "Active Widgets" section in the sidebar shows the empty state.
- **Drawer open + sidebar scroll:** Both can scroll independently. The drawer doesn't affect the sidebar's scroll position.
- **Browser fallback (dev mode):** Works identically since the drawer is pure React/DOM, no Tauri APIs.

## Testing Decisions

- **No test framework exists** for the frontend (per `.agents/agents.md`). All verification is manual.
- No tests will be written for this feature. The modules are thin UI orchestration with no isolated business logic worth testing.

## Out of Scope

- Dragging widgets from the drawer onto the canvas (adds via click only)
- Categorization or search within the drawer (will be needed when ~30 widgets exist, but deferred)
- Resizable drawer width
- Drawer on the right side (conflicts with settings sidebar)
- Animation library (CSS transitions only)
- Collapse-on-add behavior (drawer stays open after adding a widget)

## Further Notes

- The `QUICKMENU_ITEMS` array in `@/lib/widget-icons.jsx` already contains `{ type, icon, label }` for all 10 widget types. The drawer uses this same data — no new widget definitions needed.
- The existing `addWidget(type)` from `useWidgetManager.js` creates the correct widget with defaults for each type. The drawer simply calls this function.
- When new widget types are added later, they will appear in the drawer automatically as long as they're added to `QUICKMENU_ITEMS`.
