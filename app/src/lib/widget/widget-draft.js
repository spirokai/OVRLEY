/**
 * Applies canonical live widget data patches to resolved widget definitions.
 */

import { mergeDisplayVariantUpdates, resolveActiveBackdropData, resolveActiveMetricWidgetData } from './widget-resolver'

function applyWidgetDraft(liveWidgetDrafts, widget) {
  const draft = liveWidgetDrafts[widget.id]
  if (!draft) return widget

  const data = {
    ...widget.data,
    ...draft,
    ...(draft.display_variants ? { display_variants: mergeDisplayVariantUpdates(widget.data.display_variants, draft.display_variants) } : {}),
  }
  if (widget.category === 'backdrops') {
    return { ...widget, data: resolveActiveBackdropData(data, draft.display_variants?.[data.display_type]) }
  }
  if (widget.category === 'values') return { ...widget, data: resolveActiveMetricWidgetData(data) }
  return { ...widget, data }
}

/** @param {object[]} widgets Resolved widgets. @param {Object<string, object>} liveWidgetDrafts Data patches keyed by ID. @returns {object[]} Widgets with live data applied. */
export function applyWidgetDrafts(widgets, liveWidgetDrafts) {
  return widgets.map(applyWidgetDraft.bind(null, liveWidgetDrafts))
}
