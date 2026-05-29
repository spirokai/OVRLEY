Status: done

# 05 — Widget Editor and Drawer Registration

## Parent

[Heading Tape Widget PRD](../PRD.md)

## What to build

Build the widget editor panel and register the heading widget in the widget drawer so users can add, configure, and reposition it in the overlay editor.

The editor (`HeadingWidgetEditor.jsx`) provides controls for all configurable parameters:

- **Geometry**: x, y, width, height (via the existing moveable/resize system), rotation
- **Tape scale**: `pixels_per_degree` slider or numeric input
- **Ticks**: show/hide toggles for major/minor, length percentages, thickness, color pickers (regular + cardinal), alignment dropdown (`"below"` / `"centered"`)
- **Labels**: show/hide toggles for numeric/cardinal, color pickers for each, font size, offset from ticks
- **Indicator**: show/hide toggle, style dropdown (`"chevron"` / `"highlight_bar"`), placement dropdown (`"top"` / `"bottom"` / `"both"`), color picker, size

The editor follows the same component patterns as `RouteMapWidgetEditor.jsx` and `ElevationWidgetEditor.jsx` (shadcn/ui controls, collapsible sections, live preview reactivity).

Factory defaults (`widgetDefaults.js`) provide sensible initial values matching the PRD spec: 15° major ticks, 3 minors per major, aviation-style metric-first defaults, indicator enabled with chevron style at both placements.

Register the heading widget type in:
- `QUICKMENU_ITEMS` and `WIDGET_ICONS` in `widget-icons.jsx`
- `WidgetButtonGrid.jsx` drawer grid
- The heading widget uses the `Compass` Lucide icon

The widget must serialize cleanly into template JSON and deserialize correctly when loading a template.

## Acceptance criteria

- [x] `HeadingWidgetEditor.jsx` shows all controls organized in collapsible sections (tape, ticks, labels, indicator)
- [x] All config changes update the widget preview in real time
- [x] Factory defaults match the PRD spec
- [x] Heading widget appears in the widget drawer grid with the Compass icon
- [x] Heading widget can be added to the overlay, dragged, resized, and rotated
- [x] Template save/load round-trips heading widget config without loss
- [x] Editor tests pass: defaults validation, control interaction, template serialization

## Blocked by

- [04 — Frontend Preview](./04-frontend-preview.md)
