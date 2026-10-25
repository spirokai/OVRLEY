/** Table columns in their drawn order. */
export const SPLIT_COLUMNS = ['split', 'bar', 'pace']

/** Split length units; metric units report min/km and imperial units min/mi. */
export const SPLIT_UNITS = ['m', 'km', 'ft', 'mi']

/**
 * The table always shows at least one column, so clearing the last one is ignored.
 *
 * @param {string[]} columns - Columns selected in the toggle group, in click order.
 * @returns {object|null} Patch with columns in drawn order, or null to keep the current selection.
 */
export function buildVisibleColumnsUpdate(columns) {
  if (columns.length === 0) return null
  return { visible_columns: SPLIT_COLUMNS.filter((column) => columns.includes(column)) }
}

/**
 * @param {string} raw - Split length typed into the number field.
 * @returns {object|null} Patch for a positive length, or null while the input is not one.
 */
export function buildSplitDistanceUpdate(raw) {
  const split_distance = Number(raw)
  return split_distance > 0 ? { split_distance } : null
}

/**
 * Corners round at most to half the bar height.
 *
 * @param {object} data - Splits widget data.
 * @returns {number} Largest corner radius.
 */
export function maxSplitBarCornerRadius(data) {
  return data.track_thickness / 2
}

/** Border slider range of the linear gauge, which splits bars follow. */
const SPLIT_BAR_BORDER_MAX = 6

/**
 * The border must leave a visible fill inside the bar; zero draws none.
 *
 * @param {object} data - Splits widget data.
 * @returns {number} Largest border thickness.
 */
export function maxSplitBarBorderThickness(data) {
  return Math.min(Math.max(Math.floor((data.track_thickness - 1) / 2), 0), SPLIT_BAR_BORDER_MAX)
}

/**
 * A lower bar keeps its border and corner radius within the new height.
 *
 * @param {object} data - Splits widget data.
 * @param {number} track_thickness - Requested bar height.
 * @returns {object} Bar height patch.
 */
export function buildSplitBarHeightUpdate(data, track_thickness) {
  const next = { ...data, track_thickness }
  return {
    track_thickness,
    track_corner_radius: Math.min(data.track_corner_radius, maxSplitBarCornerRadius(next)),
    track_border_thickness: Math.min(data.track_border_thickness, maxSplitBarBorderThickness(next)),
  }
}
