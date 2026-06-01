/**
 * Widget behavior helpers shared across preview and editor layers.
 */

/**
 * Returns whether a widget display type should use the metric text path.
 *
 * @param {string|undefined|null} displayType - Persisted widget display type.
 * @returns {boolean} True when the widget should use text/value behavior.
 */
export function isTextDisplayType(displayType) {
  return displayType === undefined || displayType === null || displayType === 'text'
}

/**
 * Returns whether a widget is a heading widget currently in tape mode.
 *
 * @param {object|null|undefined} widget - Widget definition.
 * @returns {boolean} True when the widget should use heading tape behavior.
 */
export function isHeadingTapeWidget(widget) {
  return widget?.type === 'heading' && widget?.data?.display_type === 'heading_tape'
}

/**
 * Returns whether a widget should behave like a plot in the editor.
 *
 * Some value widgets can opt into non-text display modes. Those still live in
 * the `values` container, but their geometry and interaction model match plots.
 *
 * @param {object|null|undefined} widget - Widget definition.
 * @returns {boolean} True when the widget should use plot-style bounds/resize.
 */
export function isPlotLikeWidget(widget) {
  return widget?.category === 'plots' || (widget?.category === 'values' && !isTextDisplayType(widget?.data?.display_type))
}
