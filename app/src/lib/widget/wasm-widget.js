/** @param {object} widget Resolved WASM widget. @returns {boolean} Whether it depends on the playhead or activity. */
export function isTimeDependentWasmWidget(widget) {
  return widget.type !== 'label' && widget.type !== 'backdrop' && widget.type !== 'raster'
}
