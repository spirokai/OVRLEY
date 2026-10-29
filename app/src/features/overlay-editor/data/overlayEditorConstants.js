/**
 * Consolidated constants for the overlay-editor feature.
 * Scene grid, gesture handles, backgrounds, and viewport limits.
 */

export const EDITOR_GRID_DIVISIONS = 72

export const CORNER_RESIZE_DIRECTIONS = ['nw', 'ne', 'se', 'sw']
export const EDGE_RESIZE_DIRECTIONS = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']
export const MOVEABLE_ZOOM = 1.5
// Handles keep full size from this editor viewport width and shrink with it below, down to the minimum scale.
export const MOVEABLE_FULL_SIZE_VIEWPORT_WIDTH = 1600
export const MOVEABLE_MIN_HANDLE_SCALE = 0.5

export const CANVAS_BACKGROUND_COLORS = {
  black: '#000000',
  checker: '#000000',
  white: '#f4ead2',
  video: '#000000',
  image: '#000000',
}

export const VIEWPORT_PADDING = 72
export const ZOOM_MIN = 0.35
export const ZOOM_MAX = 4

export const PROJECT_STATUS_TRANSLATIONS = {
  Unsaved: { key: 'overlay-editor.statusUnsaved', defaultLabel: 'Unsaved' },
  Saved: { key: 'overlay-editor.statusSaved', defaultLabel: 'Saved' },
  Modified: { key: 'overlay-editor.statusModified', defaultLabel: 'Modified' },
}

export const SNAP_DIRECTIONS = { left: true, top: true, right: true, bottom: true, center: true, middle: true }
