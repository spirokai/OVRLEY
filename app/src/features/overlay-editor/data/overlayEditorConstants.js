/**
 * Consolidated constants for the overlay-editor feature.
 * Extracted from 7 source files — all data-only, no function definitions.
 */

// ---- From constants.js ----
export const DEFAULT_GRADIENT_TRIANGLE_WIDTH = 72
export const EDITOR_GRID_DIVISIONS = 72

// ---- From OverlayMoveable.jsx ----
export const CORNER_RESIZE_DIRECTIONS = ['nw', 'ne', 'se', 'sw']
export const EDGE_RESIZE_DIRECTIONS = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']
export const MOVEABLE_ZOOM = 1.5

// ---- From createOverlayMoveableHandlers.js ----
export const AXIS_LOCK_THRESHOLD = 3

// ---- From OverlayCanvas.jsx ----
export const CANVAS_BACKGROUND_COLORS = {
  black: '#000000',
  checker: '#000000',
  white: '#f4ead2',
  // for pixel parity debugging purposes
  transparent: 'transparent',
}

// ---- From metricTextUtils.js ----
export const METRIC_WIDGET_LINE_HEIGHT = 0.92
export const METRIC_WIDGET_OUTER_GAP_PX = 8
export const METRIC_WIDGET_UNITS_GAP_PX = 8
export const GRADIENT_WIDGET_TRIANGLE_GAP_PX = 8
export const GRADIENT_ZERO_EPSILON = 0.05
export const MAX_GRADIENT_ABS_PERCENT = 25
export const GRADIENT_ZERO_LINE_WIDTH_PX = 1
export const NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT = '0123456789-:.%'

// ---- From geometryUtils.js ----
export const GEOMETRY_EPSILON = 1e-9
export const ROUTE_FALLBACK_INSET_MAX_RATIO = 0.45
export const ELEVATION_FALLBACK_PADDING = 18
export const SIMPLIFY_MIN_TOLERANCE = 0.05
export const DENSITY_CLAMP_MIN = 0.1
export const DENSITY_CLAMP_MAX = 2
export const VERTICAL_SCALE_CLAMP_MIN = 0.2
export const VERTICAL_SCALE_CLAMP_MAX = 4
export const SIMPLIFY_TOLERANCE_CLAMP_MAX = 8

// ---- From useOverlayEditorState.js ----
export const VIEWPORT_PADDING = 72
export const ZOOM_MIN = 0.35
export const ZOOM_MAX = 4
export const ZOOM_DELTA = 0.05
