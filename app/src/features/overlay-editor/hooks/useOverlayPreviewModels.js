import { useMemo } from 'react'
import { buildMetricWidgetPreviewModel } from '@/features/widget-preview/widgets/metric/model'
import { buildLapTimerPreviewModel } from '@/features/widget-preview/widgets/lap-timer/model'
import { buildTextWidgetPreviewModel } from '@/features/widget-preview/widgets/text/model'
import { isBoxedDisplayType } from '@/lib/widget/standard-metrics'
import { getPreviewFontFamily } from '@/features/widget-preview/shared/textMeasurement'
import { useFontMetrics } from '@/features/widget-preview/shared/useFontMetrics'

const EMPTY_PREVIEW_MODELS = {}

function buildPreviewModels({ renderedWidgets, category, activity, previewSecond }) {
  const models = {}

  for (const widget of renderedWidgets) {
    if (widget.category !== category) continue

    const model =
      category === 'values'
        ? widget.data.display_type === 'lap_timer'
          ? buildLapTimerPreviewModel({ widget, activity, previewSecond })
          : buildMetricWidgetPreviewModel({ widget, activity, previewSecond })
        : buildTextWidgetPreviewModel({ widget })

    if (model) models[widget.id] = model
  }

  return Object.keys(models).length ? models : EMPTY_PREVIEW_MODELS
}

function buildFontRequests(renderedWidgets) {
  return renderedWidgets
    .filter(
      (widget) =>
        widget.category === 'labels' || (widget.category === 'values' && widget.type !== 'gradient' && !isBoxedDisplayType(widget.data.display_type)),
    )
    .map((widget) => ({ fontFamily: getPreviewFontFamily(widget.data.font), fontSize: widget.data.font_size }))
}

/**
 * Builds the shared preview models used by the editor canvas, badges, and
 * selection geometry.
 *
 * @param {object} params
 * @param {object[]} params.renderedWidgets - Effective widgets currently shown by the editor.
 * @param {object|null} params.activity - Parsed activity used by metric models.
 * @param {number} params.previewSecond - Canonical preview timestamp.
 * @returns {{ metricPreviewModels: object, textPreviewModels: object }} Models keyed by widget id.
 */
export default function useOverlayPreviewModels({ renderedWidgets, activity, previewSecond }) {
  const fontRequests = useMemo(() => buildFontRequests(renderedWidgets), [renderedWidgets])
  const fontMetricsVersion = useFontMetrics(fontRequests)

  const metricPreviewModels = useMemo(
    () => buildPreviewModels({ renderedWidgets, category: 'values', activity, previewSecond }),
    // Font readiness changes canvas measurements without changing the model inputs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [activity, fontMetricsVersion, previewSecond, renderedWidgets],
  )

  const textPreviewModels = useMemo(
    () => buildPreviewModels({ renderedWidgets, category: 'labels' }),
    // Font readiness changes canvas measurements without changing the model inputs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [fontMetricsVersion, renderedWidgets],
  )

  return { metricPreviewModels, textPreviewModels }
}
