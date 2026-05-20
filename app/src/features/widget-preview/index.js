/**
 * Barrel export for the widget-preview feature.
 *
 * Public API — widgets and utilities for SVG preview rendering.
 * Internal modules import directly within the feature.
 */

export { default as WidgetPreview } from './components/WidgetPreview'

export { OverlayRouteWidget } from './components/RouteRenderer'
export { OverlayElevationWidget } from './components/ElevationRenderer'
export { OverlayMetricWidget } from './components/MetricRenderer'
export { OverlayTextWidget } from './components/TextRenderer'

export { buildMetricWidgetPreviewModel } from './utils/metricWidgetPreviewModel'
export { buildTextWidgetPreviewModel } from './utils/textWidgetPreviewModel'
