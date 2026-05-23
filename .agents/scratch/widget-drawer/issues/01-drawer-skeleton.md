Status: ready-for-agent

# Widget Drawer Skeleton — Store + Collapsible Panel

## Parent

`.agents/scratch/widget-drawer/PRD.md`

## What to build

Create the foundation for the widget drawer: a Zustand layout slice to hold open/closed state, and a `WidgetDrawer` component that shows a collapsed tab (Grid3X3 icon, rounded right corners) on the left edge of the screen below the app header. Clicking the tab toggles the drawer open — a `w-40` panel slides out via CSS `translateX` transition, spanning the full remaining viewport height. Pressing Escape also closes it. The panel is empty/placeholder at this stage (content comes in a follow-up issue).

The drawer mounts inside the main flex content area in `App.jsx`, positioned `absolute` within the `relative` flex container so it overlays the left edge of the editor canvas. It is non-modal — no backdrop, no blur, no interaction blocking.

## Acceptance criteria

- [ ] `createLayoutSlice` exists in the store with `widgetDrawerOpen` (default `false`) and `toggleWidgetDrawer()` action
- [ ] `createLayoutSlice` is composed into the store in `useStore.js`
- [ ] `WidgetDrawer` component exists and is mounted in `App.jsx` inside the main content area
- [ ] Collapsed: a thin tab (~16px) with `Grid3X3` icon is visible on the left edge, below the header, with gently rounded right corners
- [ ] Clicking the tab opens the drawer — a `w-40` panel slides out with smooth CSS transition
- [ ] The tab animates attached to the panel edge (not as a separate button)
- [ ] Clicking the tab again closes the drawer
- [ ] Pressing Escape closes the drawer
- [ ] The drawer is non-modal (no overlay/backdrop, canvas remains interactive)
- [ ] No lint errors (`pnpm lint`)
- [ ] No runtime errors in the console

## Blocked by

None — can start immediately
