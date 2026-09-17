import { useMemo } from 'react'
import { buildMetricWidgetPreviewModel } from '@/features/widget-preview/widgets/metric/model'
import { COURSE_PLOT_DEFAULTS, TEXT_DEFAULTS, TYPE_DEFAULTS } from '@/lib/widget/standard-widgets'
import { createActivitySyncInput } from '../utils/activitySyncInput'
import { VIDEO_SYNC_DIAGNOSTIC_ROUTE_HEIGHT_RATIO, VIDEO_SYNC_DIAGNOSTIC_ROUTE_WIDTH_RATIO } from '../data/videoSyncConstants'

const DIAGNOSTIC_SPEED_WIDGET = Object.freeze({
  id: 'video-sync-diagnostic-speed',
  type: 'speed',
  category: 'values',
  data: Object.freeze({
    ...TEXT_DEFAULTS,
    ...TYPE_DEFAULTS.speed,
    id: 'video-sync-diagnostic-speed',
    value: 'speed',
    display_type: 'text',
    display_unit: 'kmh',
    font: 'Inter Extrabold.ttf',
    decimals: 1,
    font_size: 96,
    show_icon: false,
    show_units: true,
    color: '#ffffff',
    unit_color: '#ffffff',
  }),
})

/**
 * Builds fixed synthetic widget models for the video-sync canvas. The widgets
 * use the existing preview pipeline and never enter project or editor state.
 *
 * @param {object} options Diagnostic inputs.
 * @param {object|null} options.activity Canonical parsed activity.
 * @param {number} options.timelineSecond Current activity timeline second.
 * @param {object|null} options.markControls Workspace-owned mark action state.
 * @param {number} options.globalScale Canvas global scale.
 * @param {number} options.exportStartSecond Activity second used by metric formatting.
 * @param {{width: number, height: number}} options.sceneSize Canonical canvas dimensions.
 * @param {boolean} options.enabled Whether the sync workspace is active.
 * @returns {object|null} Explicit render state for canvas diagnostics and mark controls.
 */
export default function useVideoSyncDiagnostics({ activity, timelineSecond, markControls, globalScale, exportStartSecond, sceneSize, enabled }) {
  const availability = useMemo(() => (enabled ? createActivitySyncInput(activity).availability : null), [activity, enabled])
  const speedPreviewModel = useMemo(
    () =>
      enabled
        ? buildMetricWidgetPreviewModel({
            widget: DIAGNOSTIC_SPEED_WIDGET,
            activity,
            previewSecond: timelineSecond,
            exportStartSecond,
            globalScale,
          })
        : null,
    [activity, enabled, exportStartSecond, globalScale, timelineSecond],
  )
  const routeWidget = useMemo(
    () =>
      enabled
        ? {
            id: 'video-sync-diagnostic-route',
            type: 'course',
            category: 'plots',
            data: {
              ...COURSE_PLOT_DEFAULTS,
              id: 'video-sync-diagnostic-route',
              width: Math.round(sceneSize.width * VIDEO_SYNC_DIAGNOSTIC_ROUTE_WIDTH_RATIO),
              height: Math.round(sceneSize.height * VIDEO_SYNC_DIAGNOSTIC_ROUTE_HEIGHT_RATIO),
              completed_line_color: '#ffffff',
              completed_line_opacity: 100,
              remaining_line_color: '#ffffff',
              remaining_line_opacity: 100,
              completed_line_width: 15,
              remaining_line_width: 15,
              marker_color: '#ff1f1f',
              marker_size: 9,
              marker_variant: 'single',
              marker_variant_diameter: 1,
              marker_opacity: 100,
              show_full_activity: true,
            },
          }
        : null,
    [enabled, sceneSize.height, sceneSize.width],
  )

  if (!enabled) return null

  return {
    markControls,
    speed: {
      available: availability.speed,
      previewModel: speedPreviewModel,
      widget: DIAGNOSTIC_SPEED_WIDGET,
    },
    route: {
      available: availability.course,
      widget: routeWidget,
    },
  }
}
