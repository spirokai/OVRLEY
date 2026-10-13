import { isStandardMetricWidgetType, getSupportedDisplayTypes } from './standard-metrics'

/** @param {object} widget Resolved widget. @returns {boolean} Whether WASM owns its preview. */
export function isWasmWidget(widget) {
  return (
    widget.type === 'raster' ||
    widget.type === 'backdrop' ||
    widget.type === 'label' ||
    (widget.type === 'time' && widget.data.display_type === 'text') ||
    (isStandardMetricWidgetType(widget.type) && widget.data.display_type === 'text' && getSupportedDisplayTypes(widget.type).includes('text'))
  )
}

/** @param {object} widget Resolved WASM widget. @returns {boolean} Whether it depends on the playhead or activity. */
export function isTimeDependentWasmWidget(widget) {
  return widget.type !== 'label' && widget.type !== 'backdrop' && widget.type !== 'raster'
}
