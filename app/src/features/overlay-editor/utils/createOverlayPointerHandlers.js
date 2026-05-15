/**
 * Provides overlay editor helpers for create overlay pointer handlers.
 */

import { ZOOM_MIN, ZOOM_MAX } from '../data/overlayEditorConstants'
import { clamp } from '@/lib/geometryUtils'
import { buildSelectionRect, getPrimarySelectionId, hasSelectionModifier, normalizeSelectionIds, rectanglesIntersect } from './overlayEditorHelpers'

/**
 * Converts a client-space mouse position to scene-space coordinates,
 * clamped to the scene bounds.
 *
 * @param {HTMLElement|null} sceneElement - The scene container DOM element.
 * @param {number} displayScale - Current display scale factor.
 * @param {{ width: number, height: number }} sceneSize - Scene dimensions.
 * @param {number} clientX - Client-space X.
 * @param {number} clientY - Client-space Y.
 * @returns {{ x: number, y: number }|null} Scene-space point or null.
 */
function getScenePoint(sceneElement, displayScale, sceneSize, clientX, clientY) {
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
 * Finds widget IDs whose DOM bounding rects intersect with the given
 * scene-space selection rectangle. Used for marquee/selection box.
 *
 * @param {object} options
 * @param {number} options.displayScale - Display scale for coordinate conversion.
 * @param {{ x: number, y: number, width: number, height: number }} options.nextSelectionRect - Selection rect in scene coords.
 * @param {string[]} options.orderedWidgetIds - Ordered list of all widget IDs.
 * @param {HTMLElement|null} options.sceneElement - Scene container element.
 * @param {Object<string, HTMLElement>} options.widgetNodes - Widget DOM node map.
 * @returns {string[]} Intersecting widget IDs.
 */
function getIntersectedWidgetIds({ displayScale, nextSelectionRect, orderedWidgetIds, sceneElement, widgetNodes }) {
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
 * Creates pointer event handlers for the overlay editor — scene click/marquee
 * selection, widget mouse down (single and group), and mouse wheel zoom.
 *
 * @param {object} options
 * @param {Function} options.commitSelection - Commits a selection set to state and store.
 * @param {number} options.displayScale - Current display scale.
 * @param {React.RefObject} options.moveableRef - Ref to Moveable instance for programmatic dragStart.
 * @param {React.MutableRefObject} options.marqueeCleanupRef - Ref for cleanup function.
 * @param {React.MutableRefObject} options.marqueeSelectionRef - Ref for marquee gesture state.
 * @param {Function} options.onZoomLevelChange - Zoom level state setter.
 * @param {string[]} options.orderedWidgetIds - Ordered widget ID list.
 * @param {HTMLElement|null} options.sceneElement - Scene container element.
 * @param {{ width: number, height: number }} options.sceneSize - Scene dimensions.
 * @param {string|null} options.selectedWidgetId - Currently selected widget ID.
 * @param {string[]} options.selectedWidgetIds - All selected widget IDs.
 * @param {Function} options.setGroupDragSelectionIds - Setter for group-drag IDs.
 * @param {Function} options.setIsGroupDragActive - Setter for group-drag flag.
 * @param {Function} options.setSelectionRect - Setter for selection rectangle.
 * @param {Function} options.setSelectionState - Setter for widget selection.
 * @param {Object<string, HTMLElement>} options.widgetNodes - Widget DOM node map.
 * @returns {{ handleSceneMouseDown: Function, handleWheel: Function, handleWidgetMouseDown: Function }}
 *   Pointer event handlers.
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
    const delta = event.deltaY < 0 ? 0.05 : -0.05
    onZoomLevelChange((current) => clamp(Number((current + delta).toFixed(2)), ZOOM_MIN, ZOOM_MAX))
  }

  const handleWidgetMouseDown = (event, widgetId) => {
    event.stopPropagation()

    if (event.button !== 0) {
      return
    }

    const isSelected = selectedWidgetIds.includes(widgetId)
    const isCtrlAxisLockDrag = event.ctrlKey && isSelected && !event.metaKey && !event.shiftKey

    if (isCtrlAxisLockDrag && selectedWidgetIds.length > 1) {
      event.preventDefault()
      const draggedWidgetIds = [...selectedWidgetIds]
      setIsGroupDragActive(true)
      setGroupDragSelectionIds(draggedWidgetIds)
      moveableRef.current?.dragStart(event.nativeEvent, event.currentTarget)
      return
    }

    if (isCtrlAxisLockDrag) {
      return
    }

    if (hasSelectionModifier(event)) {
      const nextIds = isSelected ? selectedWidgetIds.filter((selectedId) => selectedId !== widgetId) : [...selectedWidgetIds, widgetId]
      const nextPrimaryId = isSelected ? getPrimarySelectionId(nextIds, selectedWidgetId === widgetId ? null : selectedWidgetId) : widgetId

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

    const startPoint = getScenePoint(sceneElement, displayScale, sceneSize, event.clientX, event.clientY)
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
      const nextPoint = getScenePoint(sceneElement, displayScale, sceneSize, moveEvent.clientX, moveEvent.clientY)
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
      const nextIds = additive ? normalizeSelectionIds([...baseIds, ...hitIds], orderedWidgetIds) : hitIds

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

      commitSelection(gesture.previewIds, getPrimarySelectionId(gesture.previewIds, selectedWidgetId))
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
