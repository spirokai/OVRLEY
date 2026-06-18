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
export { OverlayHeadingWidget } from './components/HeadingRenderer'
export { buildMetricWidgetPreviewModel } from './utils/metricWidgetPreviewUtils'

export { buildTextWidgetPreviewModel } from './utils/textWidgetPreviewUtils'
