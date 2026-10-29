/** Rectangle backdrop edge strip rules shared by the inspector. */

/** Canonical edge order stored in `edge_strips`. */
export const RECTANGLE_EDGE_KEYS = ['top', 'right', 'bottom', 'left']

/**
 * @param {string[]} edges Active edge strips.
 * @param {string} edge Edge to toggle.
 * @returns {string[]} Active edges in canonical order.
 */
export function toggleEdgeStrip(edges, edge) {
  return RECTANGLE_EDGE_KEYS.filter((key) => (key === edge ? !edges.includes(key) : edges.includes(key)))
}

/**
 * Strips are drawn inward, so opposing strips may meet but not overlap.
 * @param {number} width Rectangle width.
 * @param {number} height Rectangle height.
 * @returns {number} Largest whole-pixel strip thickness.
 */
export function maxEdgeStripThickness(width, height) {
  return Math.floor(Math.min(width, height) / 2)
}
