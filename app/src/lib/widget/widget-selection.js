/**
 * Filters and orders the selection set to match the canonical widget order,
 * removing any IDs that no longer exist in the widget list.
 *
 * @param {string[]} widgetIds - Unordered selected widget IDs.
 * @param {string[]} orderedWidgetIds - Reference-order list of all widget IDs.
 * @returns {string[]} Filtered IDs in canonical order.
 */
export function normalizeSelectionIds(widgetIds, orderedWidgetIds) {
  const idSet = new Set(widgetIds)
  return orderedWidgetIds.filter((widgetId) => idSet.has(widgetId))
}

/**
 * Returns the primary selection ID from a list — prefers the given preferredId
 * if it exists in the list, otherwise returns the last widget ID.
 *
 * @param {string[]} widgetIds - Current selection list.
 * @param {string|null} [preferredId=null] - Preferred primary ID.
 * @returns {string|null} Primary widget ID or null if selection is empty.
 */
export function getPrimarySelectionId(widgetIds, preferredId = null) {
  if (preferredId && widgetIds.includes(preferredId)) {
    return preferredId
  }

  return widgetIds[widgetIds.length - 1] ?? null
}
