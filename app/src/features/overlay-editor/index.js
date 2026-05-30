/**
 * Barrel export for the overlay-editor feature.
 *
 * Public API — only components and utilities intended for cross-feature use
 * are exported here. Internal modules import directly within the feature.
 */

export { default as OverlayEditor } from './components/OverlayEditor'

export {
  timeToSeconds,
  buildScopedRouteSamples,
  buildScopedElevationSeries,
  resolveExportRangeWindow,
  getWindowProgressAtTime,
  getActivityDurationSeconds,
  getExportWindowDistanceSpan,
  normalizeDistanceProgressToWindow,
} from '@/lib/export-range'

export {
  getEffectivePreviewFps,
  getInterpolatedActivityValue,
  getInterpolatedTimeValue,
  getDistanceProgressAtElapsed,
  getSeriesValueAtProgress,
  getInterpolatedSeriesValue,
} from './utils/overlayEditorUtils'

export * from './data/overlayEditorConstants'

export { FONT_FAMILY_MAP, WIDGET_ICONS, DEFAULT_ACTIVITY_PREVIEW } from './data/overlayEditorConfig'

export { METRIC_ICON_SVGS } from './data/metricWidgetAssets'
