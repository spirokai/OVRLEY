/**
 * @file display-type-behavior – Widget-instance predicates that bridge
 * display-type definitions to widget data shapes.
 *
 * This module depends on the shared manifest reader (standard-metrics.js)
 * but lives in its own file to keep manifest/domain logic and
 * widget-instance behavior concerns separate.
 *
 * @module display-type-behavior
 */

import { isBoxedDisplayType, isStandardMetricWidgetType } from './standard-metrics'

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
 * Returns whether a widget uses a boxed (framed) display type that should
 * behave like a bounded visual frame rather than intrinsic text.
 *
 * For standard metric widgets, boxed vs intrinsic is derived from the
 * display_type in the shared manifest — not from the legacy container/category.
 * For non-metric widgets (route, gradient, etc.), category === 'plots' still
 * drives the boxed behavior.
 *
 * @param {object|null|undefined} widget - Widget definition.
 * @returns {boolean} True when the widget should use boxed layout/resize behavior.
 */
export function isBoxedMetricWidget(widget) {
  if (!widget) return false
  if (isStandardMetricWidgetType(widget.type)) {
    return isBoxedDisplayType(widget?.data?.display_type)
  }
  return widget?.category === 'plots'
}
