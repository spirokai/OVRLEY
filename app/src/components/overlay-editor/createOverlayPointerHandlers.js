/**
 * Provides overlay editor helpers for create overlay pointer handlers.
 */

import { clamp } from './utils'
import {
  buildSelectionRect,
  getPrimarySelectionId,
  hasSelectionModifier,
  normalizeSelectionIds,
  rectanglesIntersect,
} from './overlayEditorHelpers'

/**
 * Returns scene point.
 *
 * @param {*} sceneElement - Value for scene element.
 * @param {*} displayScale - Value for display scale.
 * @param {*} sceneSize - Numeric scene size value.
 * @param {*} clientX - Value for client x.
 * @param {*} clientY - Value for client y.
 * @returns {object} Requested value or structure.
 */
function getScenePoint(
  sceneElement,
  displayScale,
  sceneSize,
  clientX,
  clientY,
) {
  if (!sceneElement) {
    return null
  }

  const sceneBounds = sceneElement.getBoundingClientRect()
  return {
    x: clamp((clientX - sceneBounds.left) / displayScale, 0, sceneSize.width),
    y: clamp((clientY - sceneBounds.top) / displayScale, 0, sceneSize.height),
  }
}

/**
 * Returns intersected widget ids.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.displayScale - Value for display scale.
 * @param {*} options.nextSelectionRect - Value for next selection rect.
 * @param {*} options.orderedWidgetIds - Value for ordered widget ids.
 * @param {*} options.sceneElement - Value for scene element.
 * @param {*} options.widgetNodes - Value for widget nodes.
 * @returns {*} Requested value or structure.
 */
function getIntersectedWidgetIds({
  displayScale,
  nextSelectionRect,
  orderedWidgetIds,
  sceneElement,
  widgetNodes,
}) {
  if (!sceneElement) {
    return []
  }

  const sceneBounds = sceneElement.getBoundingClientRect()

  return orderedWidgetIds.filter((widgetId) => {
    const node = widgetNodes[widgetId]
    if (!node) {
      return false
    }

    const nodeBounds = node.getBoundingClientRect()
    const nodeRect = {
      x: (nodeBounds.left - sceneBounds.left) / displayScale,
      y: (nodeBounds.top - sceneBounds.top) / displayScale,
      width: nodeBounds.width / displayScale,
      height: nodeBounds.height / displayScale,
    }

    return rectanglesIntersect(nextSelectionRect, nodeRect)
  })
}

/**
 * Provides overlay pointer handlers state and actions.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.commitSelection - Value for commit selection.
 * @param {*} options.displayScale - Value for display scale.
 * @param {*} options.moveableRef - Value for moveable ref.
 * @param {*} options.marqueeCleanupRef - Value for marquee cleanup ref.
 * @param {*} options.marqueeSelectionRef - Value for marquee selection ref.
 * @param {*} options.onZoomLevelChange - Callback invoked to zoom level change.
 * @param {*} options.orderedWidgetIds - Value for ordered widget ids.
 * @param {*} options.sceneElement - Value for scene element.
 * @param {*} options.sceneSize - Numeric scene size value.
 * @param {*} options.selectedWidgetId - Value for selected widget id.
 * @param {*} options.selectedWidgetIds - Value for selected widget ids.
 * @param {*} options.setGroupDragSelectionIds - Value for set group drag selection ids.
 * @param {*} options.setIsGroupDragActive - Value for set is group drag active.
 * @param {*} options.setSelectionRect - Value for set selection rect.
 * @param {*} options.setSelectionState - Value for set selection state.
 * @param {*} options.widgetNodes - Value for widget nodes.
 * @returns {object} Result produced by the helper.
 */
export default function useOverlayPointerHandlers({
  commitSelection,
  displayScale,
  moveableRef,
  marqueeCleanupRef,
  marqueeSelectionRef,
  onZoomLevelChange,
  orderedWidgetIds,
  sceneElement,
  sceneSize,
  selectedWidgetId,
  selectedWidgetIds,
  setGroupDragSelectionIds,
  setIsGroupDragActive,
  setSelectionRect,
  setSelectionState,
  widgetNodes,
}) {
  const handleWheel = (event) => {
    event.preventDefault()
    const delta = event.deltaY < 0 ? 0.1 : -0.1
    onZoomLevelChange((current) =>
      clamp(Number((current + delta).toFixed(2)), 0.35, 4),
    )
  }

  const handleWidgetMouseDown = (event, widgetId) => {
    event.stopPropagation()

    if (event.button !== 0) {
      return
    }

    const isSelected = selectedWidgetIds.includes(widgetId)
    if (hasSelectionModifier(event)) {
      const nextIds = isSelected
        ? selectedWidgetIds.filter((selectedId) => selectedId !== widgetId)
        : [...selectedWidgetIds, widgetId]
      const nextPrimaryId = isSelected
        ? getPrimarySelectionId(
            nextIds,
            selectedWidgetId === widgetId ? null : selectedWidgetId,
          )
        : widgetId

      commitSelection(nextIds, nextPrimaryId)
      return
    }

    if (isSelected && selectedWidgetIds.length > 1) {
      event.preventDefault()
      const draggedWidgetIds = [...selectedWidgetIds]
      setIsGroupDragActive(true)
      setGroupDragSelectionIds(draggedWidgetIds)
      moveableRef.current?.dragStart(event.nativeEvent, event.currentTarget)
      return
    }

    if (selectedWidgetIds.length === 1 && selectedWidgetId === widgetId) {
      return
    }

    commitSelection([widgetId], widgetId)
  }

  const handleSceneMouseDown = (event) => {
    if (event.button !== 0) {
      return
    }

    const startPoint = getScenePoint(
      sceneElement,
      displayScale,
      sceneSize,
      event.clientX,
      event.clientY,
    )
    if (!startPoint) {
      return
    }

    event.preventDefault()
    marqueeCleanupRef.current?.()

    const additive = hasSelectionModifier(event)
    const baseIds = additive ? selectedWidgetIds : []
    marqueeSelectionRef.current = {
      additive,
      baseIds,
      hasMoved: false,
      previewIds: baseIds,
      startPoint,
    }

    setSelectionRect({
      x: startPoint.x,
      y: startPoint.y,
      width: 0,
      height: 0,
    })

    const handleWindowMouseMove = (moveEvent) => {
      const nextPoint = getScenePoint(
        sceneElement,
        displayScale,
        sceneSize,
        moveEvent.clientX,
        moveEvent.clientY,
      )
      const gesture = marqueeSelectionRef.current
      if (!nextPoint || !gesture) {
        return
      }

      const nextRect = buildSelectionRect(gesture.startPoint, nextPoint)
      const hasMoved = nextRect.width > 2 || nextRect.height > 2
      const hitIds = hasMoved
        ? getIntersectedWidgetIds({
            displayScale,
            nextSelectionRect: nextRect,
            orderedWidgetIds,
            sceneElement,
            widgetNodes,
          })
        : []
      const nextIds = additive
        ? normalizeSelectionIds([...baseIds, ...hitIds], orderedWidgetIds)
        : hitIds

      marqueeSelectionRef.current = {
        ...gesture,
        hasMoved,
        previewIds: nextIds,
      }

      setSelectionRect(nextRect)
      if (hasMoved) {
        setSelectionState(nextIds)
      }
    }

    const handleWindowMouseUp = () => {
      const gesture = marqueeSelectionRef.current

      marqueeCleanupRef.current?.()
      marqueeSelectionRef.current = null
      setSelectionRect(null)

      if (!gesture) {
        return
      }

      if (!gesture.hasMoved) {
        commitSelection(gesture.baseIds, selectedWidgetId)
        return
      }

      commitSelection(
        gesture.previewIds,
        getPrimarySelectionId(gesture.previewIds, selectedWidgetId),
      )
    }

    marqueeCleanupRef.current = () => {
      window.removeEventListener('mousemove', handleWindowMouseMove)
      window.removeEventListener('mouseup', handleWindowMouseUp)
      marqueeCleanupRef.current = null
    }

    window.addEventListener('mousemove', handleWindowMouseMove)
    window.addEventListener('mouseup', handleWindowMouseUp)
  }

  return {
    handleSceneMouseDown,
    handleWheel,
    handleWidgetMouseDown,
  }
}
