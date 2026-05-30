/**
 * @file useWidgetSelection – Selection management for the overlay editor.
 *
 * Owns the selection reconciliation layer between the zustand store
 * (canonical owner of selectedWidgetId / selectedWidgetIds) and the
 * editor surfaces (canvas + sidebar). Handles intentful selection
 * updates (single-select, toggle, marquee) and group-drag reconciliation.
 *
 * Used by both the overlay canvas and the widget sidebar via
 * composition at the component level (OverlayEditor.jsx).
 *
 * @module useWidgetSelection
 */

import { useCallback, useMemo } from 'react'
import useStore from '@/store/useStore'
import { getPrimarySelectionId, normalizeSelectionIds } from '../utils/overlayEditorHelpers'

export default function useWidgetSelection({
  orderedWidgetIds,
  renderedWidgetMap,
  widgetNodes,
  isGroupDragActive = false,
  groupDragSelectionIds = [],
}) {
  const selectedWidgetId = useStore((state) => state.selectedWidgetId)
  const selectedWidgetIds = useStore((state) => state.selectedWidgetIds)
  const setWidgetSelection = useStore((state) => state.setWidgetSelection)

  const setSelectionState = useCallback(
    (widgetIds) => {
      setWidgetSelection(normalizeSelectionIds(widgetIds, orderedWidgetIds))
    },
    [orderedWidgetIds, setWidgetSelection],
  )

  const commitSelection = useCallback(
    (widgetIds, preferredId = null) => {
      const normalizedIds = normalizeSelectionIds(widgetIds, orderedWidgetIds)
      setWidgetSelection(normalizedIds, getPrimarySelectionId(normalizedIds, preferredId))
    },
    [orderedWidgetIds, setWidgetSelection],
  )

  const effectiveSelectedWidgetIds = useMemo(
    () => (isGroupDragActive ? normalizeSelectionIds(groupDragSelectionIds, orderedWidgetIds) : selectedWidgetIds),
    [groupDragSelectionIds, isGroupDragActive, orderedWidgetIds, selectedWidgetIds],
  )

  const selectedWidgets = useMemo(
    () => effectiveSelectedWidgetIds.map((widgetId) => renderedWidgetMap[widgetId]).filter(Boolean),
    [effectiveSelectedWidgetIds, renderedWidgetMap],
  )

  const primarySelectedWidgetId = getPrimarySelectionId(effectiveSelectedWidgetIds, selectedWidgetId)
  const selectedWidget = primarySelectedWidgetId ? renderedWidgetMap[primarySelectedWidgetId] || null : null
  const isGroupSelection = effectiveSelectedWidgetIds.length > 1

  const selectedTarget = !isGroupSelection && primarySelectedWidgetId ? widgetNodes[primarySelectedWidgetId] || null : null

  const selectedTargets = useMemo(
    () => (isGroupSelection ? effectiveSelectedWidgetIds.map((widgetId) => widgetNodes[widgetId]).filter(Boolean) : []),
    [effectiveSelectedWidgetIds, isGroupSelection, widgetNodes],
  )

  const elementGuidelines = useMemo(() => {
    const allIds = Object.keys(renderedWidgetMap)
    return allIds
      .filter((id) => !effectiveSelectedWidgetIds.includes(id))
      .map((id) => widgetNodes[id])
      .filter(Boolean)
  }, [effectiveSelectedWidgetIds, renderedWidgetMap, widgetNodes])

  return {
    commitSelection,
    effectiveSelectedWidgetIds,
    elementGuidelines,
    isGroupSelection,
    primarySelectedWidgetId,
    selectedTarget,
    selectedTargets,
    selectedWidget,
    selectedWidgetId,
    selectedWidgetIds,
    selectedWidgets,
    setSelectionState,
    setWidgetSelection,
  }
}
