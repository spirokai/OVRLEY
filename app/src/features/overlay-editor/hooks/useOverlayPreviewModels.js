import { useMemo } from 'react'
import { buildMetricWidgetPreviewModel } from '@/features/widget-preview/widgets/metric/model'
import { buildLapTimerPreviewModel, prepareLapLogPreview } from '@/features/widget-preview/widgets/lap-timer/model'
import { buildTextWidgetPreviewModel } from '@/features/widget-preview/widgets/text/model'

const EMPTY_PREVIEW_MODELS = {}

function buildPreviewModels({ renderedWidgets, category, activity, previewSecond, lapLogPreparations, globalScale, exportStartSecond }) {
  const models = {}

  for (const widget of renderedWidgets) {
    if (widget.category !== category) continue

    const model =
      category === 'values'
        ? widget.data.display_type === 'lap_timer'
          ? buildLapTimerPreviewModel({ widget, activity, previewSecond, lapLogPreparation: lapLogPreparations[widget.id] })
          : buildMetricWidgetPreviewModel({ widget, activity, previewSecond, globalScale, exportStartSecond })
        : buildTextWidgetPreviewModel({ widget })

    if (model) models[widget.id] = model
  }

  return Object.keys(models).length ? models : EMPTY_PREVIEW_MODELS
}

function prepareLapLogs(renderedWidgets, activity) {
  const preparations = {}
  for (const widget of renderedWidgets) {
    if (widget.category === 'values' && widget.data.display_type === 'lap_timer' && widget.data.lap_timer_mode === 'lap_log') {
      preparations[widget.id] = prepareLapLogPreview({ widget, activity })
    }
  }
  return preparations
}

/**
 * Builds the shared preview models used by the editor canvas, badges, and
 * selection geometry.
 *
 * @param {object} params
 * @param {object[]} params.renderedWidgets - Effective widgets currently shown by the editor.
 * @param {object|null} params.activity - Parsed activity used by metric models.
 * @param {number} params.previewSecond - Canonical preview timestamp.
 * @param {number} params.exportStartSecond - Canonical timeline second where the current export begins.
 * @param {number} params.globalScale - Scale applied to intrinsic widget previews.
 * @returns {{ metricPreviewModels: object, textPreviewModels: object }} Models keyed by widget id.
 */
export default function useOverlayPreviewModels({ renderedWidgets, activity, previewSecond, exportStartSecond, globalScale }) {
  const lapLogPreparations = useMemo(() => prepareLapLogs(renderedWidgets, activity), [activity, renderedWidgets])

  const metricPreviewModels = useMemo(
    () =>
      buildPreviewModels({
        renderedWidgets,
        category: 'values',
        activity,
        previewSecond,
        lapLogPreparations,
        globalScale,
        exportStartSecond,
      }),
    [activity, exportStartSecond, globalScale, lapLogPreparations, previewSecond, renderedWidgets],
  )

  const textPreviewModels = useMemo(() => buildPreviewModels({ renderedWidgets, category: 'labels' }), [renderedWidgets])

  return { metricPreviewModels, textPreviewModels }
}
