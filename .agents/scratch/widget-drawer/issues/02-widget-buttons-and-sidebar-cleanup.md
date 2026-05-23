Status: ready-for-agent

# Widget Buttons + Sidebar Cleanup

## Parent

`.agents/scratch/widget-drawer/PRD.md`

## What to build

Populate the empty drawer panel from issue 01 with a 2-column scrollable grid of widget buttons. Each button is 52×52px with the widget's icon centered above its full name in tiny font. The buttons use the existing `QUICKMENU_ITEMS` data from `@/lib/widget-icons` — the same 10 widget types (Text, Speed, Heart Rate, Power, Cadence, Time, Temperature, Gradient, Elevation, Route Map).

Clicking a button calls `addWidget(type)` from `useWidgetManager` and keeps the drawer open (does not auto-collapse). No new widget definitions are needed — the existing factory functions create the correct defaults per type.

Remove the old quick-add toolbar from `SidebarWidgetsTab.jsx` (the 5-column icon-only grid at the top of the Widgets tab). The "Active Widgets" accordion and per-widget editors remain unchanged.

## Acceptance criteria

- [ ] Drawer panel shows a scrollable 2-column grid of widget buttons
- [ ] Each button is 52×52px (`h-13 w-13`) with its type-specific icon and full widget name label below in tiny font
- [ ] Labels use full names from `TYPE_LABELS` (e.g. "Heart Rate", "Temperature") not abbreviations
- [ ] Long labels do not break the layout (use tiny font with appropriate padding)
- [ ] Clicking any widget button adds that widget type to the config and keeps the drawer open
- [ ] Newly added widgets appear in the sidebar's "Active Widgets" accordion immediately
- [ ] The old 5-column quick-add toolbar is removed from `SidebarWidgetsTab.jsx`
- [ ] The "Active Widgets" accordion and widget editors in the sidebar remain fully functional
- [ ] No lint errors (`pnpm lint`)
- [ ] No runtime errors in the console

## Blocked by

- `.agents/scratch/widget-drawer/issues/01-drawer-skeleton.md`
