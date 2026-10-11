/** @param {object} widget Resolved widget. @returns {boolean} Whether WASM owns its preview. */
export function isWasmWidget(widget) {
  return widget.type === 'speed' && widget.data.display_type === 'text'
}
