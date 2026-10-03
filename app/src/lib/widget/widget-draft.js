/**
 * Shared live widget draft transformations.
 */

import { mergeDisplayVariantUpdates, resolveActiveBackdropData, resolveActiveMetricWidgetData } from './widget-resolver'

function resolveDraftData(widget, draftData) {
  const mergedData = {
    ...widget.data,
    ...draftData,
    ...(draftData.display_variants
      ? {
          display_variants: mergeDisplayVariantUpdates(widget.data.display_variants, draftData.display_variants),
        }
      : {}),
  }

  if (widget.category === 'backdrops') {
    return resolveActiveBackdropData(mergedData)
  }

  if (widget.category === 'values') {
    return resolveActiveMetricWidgetData(mergedData)
  }

  return mergedData
}

function getCanvasDraftData(draft) {
  if (draft.layout?.mode === 'scale') return null
  if (!draft.layout) return draft.data

  const renderData = Object.fromEntries(Object.entries(draft.data).filter(([key]) => key !== 'x' && key !== 'y'))
  return Object.keys(renderData).length ? renderData : null
}

/**
 * Applies partial live data drafts to resolved widgets.
 *
 * @param {Array<object>} widgets - Resolved widget definitions.
 * @param {Object<string, object>} liveWidgetDrafts - Drafts keyed by widget ID.
 * @returns {Array<object>} Widgets with live data applied.
 */
export function applyWidgetDrafts(widgets, liveWidgetDrafts) {
  return widgets.map((widget) => {
    const draft = liveWidgetDrafts[widget.id]
    return draft ? { ...widget, data: resolveDraftData(widget, draft.data) } : widget
  })
}

/**
 * Applies render-relevant live data drafts to canvas widgets. Direct
 * interaction layouts continue to own the outer DOM geometry, while preview
 * components still need live dimensions and display settings for direct frame
 * changes. Scale layouts own the complete canvas preview until commit, while
 * other layouts omit position because the DOM layout owns that movement.
 *
 * @param {Array<object>} widgets - Resolved widget definitions.
 * @param {Object<string, object>} liveWidgetDrafts - Drafts keyed by widget ID.
 * @returns {Array<object>} Widgets with render-relevant live data applied.
 */
export function applyWidgetDraftsForCanvas(widgets, liveWidgetDrafts) {
  return widgets.map((widget) => {
    const draft = liveWidgetDrafts[widget.id]
    if (!draft) return widget

    const draftData = getCanvasDraftData(draft)
    return draftData ? { ...widget, data: resolveDraftData(widget, draftData) } : widget
  })
}

/**
 * Returns the data patch stored in a live draft.
 *
 * @param {object|null|undefined} draft - Live widget draft.
 * @returns {object} Partial widget data patch.
 */
export function getWidgetDraftData(draft) {
  return draft?.data ?? {}
}
