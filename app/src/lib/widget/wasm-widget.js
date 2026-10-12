import { isStandardMetricWidgetType, getSupportedDisplayTypes } from './standard-metrics'

/** @param {object} widget Resolved widget. @returns {boolean} Whether WASM owns its preview. */
export function isWasmWidget(widget) {
  return isStandardMetricWidgetType(widget.type) && widget.data.display_type === 'text' && getSupportedDisplayTypes(widget.type).includes('text')
}
