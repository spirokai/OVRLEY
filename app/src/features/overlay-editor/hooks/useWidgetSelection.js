/**
 * Reconciles canonical store selection with the preview widgets and mounted nodes.
 */

import { useCallback, useEffect, useEffectEvent, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import useStore from '@/store/useStore'
import { getWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'
import { getPrimarySelectionId, normalizeSelectionIds } from '@/lib/widget/widget-config'
import {
  areWidgetBadgesEqual,
  buildSelectionRect,
  buildWidgetBadges,
  getElementPoint,
  getIntersectedWidgetIds,
  getScenePoint,
  hasSelectionModifier,
} from '../utils/selectionGeometry'

function setHoveredWidgetFromEvent(setHoveredWidgetId, event) {
  const node = event.target instanceof Element ? event.target.closest('[data-widget-id]') : null
  setHoveredWidgetId(node === null ? null : node.dataset.widgetId)
}

function handleWidgetMouseDown(context, event, widgetId) {
  const { commitSelection, moveableRef, selectedWidgetId, selectedWidgetIds, setGroupDragSelectionIds, setGroupGestureType } = context
  event.stopPropagation()
  if (event.button !== 0) return

  const isSelected = selectedWidgetIds.includes(widgetId)
  const isCtrlAxisLockDrag = event.ctrlKey && isSelected && !event.metaKey && !event.shiftKey
  if (isSelected && selectedWidgetIds.length > 1 && (isCtrlAxisLockDrag || !hasSelectionModifier(event))) {
    event.preventDefault()
    setGroupGestureType('drag')
    setGroupDragSelectionIds([...selectedWidgetIds])
    moveableRef.current?.dragStart(event.nativeEvent, event.currentTarget)
    return
  }
  if (isCtrlAxisLockDrag) return
  if (hasSelectionModifier(event)) {
    const nextIds = isSelected ? selectedWidgetIds.filter(isDifferentWidget.bind(null, widgetId)) : [...selectedWidgetIds, widgetId]
    const nextPrimaryId = isSelected ? getPrimarySelectionId(nextIds, selectedWidgetId === widgetId ? null : selectedWidgetId) : widgetId
    commitSelection(nextIds, nextPrimaryId)
    return
  }
  if (selectedWidgetIds.length !== 1 || selectedWidgetId !== widgetId) commitSelection([widgetId], widgetId)
}

function isDifferentWidget(widgetId, selectedId) {
  return selectedId !== widgetId
}

function stopMarquee(marqueeRef) {
  const gesture = marqueeRef.current
  if (!gesture) return
  window.removeEventListener('mousemove', gesture.mouseMove)
  window.removeEventListener('mouseup', gesture.mouseUp)
  marqueeRef.current = null
}

function handleWindowMouseMove(context, event) {
  const { commitSelection, marqueeRef, orderedWidgetIds, setSelectionRect, stageElement } = context
  const gesture = marqueeRef.current
  if (!gesture) return
  const sceneRect = buildSelectionRect(gesture.startScenePoint, getScenePoint(context, event))
  const stageRect = buildSelectionRect(gesture.startStagePoint, getElementPoint(stageElement, event.clientX, event.clientY))
  gesture.hasMoved = stageRect.width > 2 || stageRect.height > 2
  const hitIds = gesture.hasMoved ? getIntersectedWidgetIds(context, sceneRect) : []
  gesture.previewIds = gesture.additive ? normalizeSelectionIds([...gesture.baseIds, ...hitIds], orderedWidgetIds) : hitIds
  setSelectionRect(stageRect)
  if (gesture.hasMoved) commitSelection(gesture.previewIds)
}

function handleWindowMouseUp({ commitSelection, marqueeRef, selectedWidgetId, setSelectionRect }) {
  const gesture = marqueeRef.current
  stopMarquee(marqueeRef)
  setSelectionRect(null)
  if (gesture) commitSelection(gesture.hasMoved ? gesture.previewIds : gesture.baseIds, selectedWidgetId)
}

function handleSceneMouseDown(context, event) {
  const { marqueeRef, sceneElement, selectedWidgetIds, setSelectionRect, stageElement } = context
  if (event.button !== 0 || (event.target instanceof Element && event.target.closest('.ovrley-moveable'))) return
  if (!sceneElement || !stageElement) return
  event.preventDefault()
  stopMarquee(marqueeRef)

  const additive = hasSelectionModifier(event)
  const baseIds = additive ? selectedWidgetIds : []
  const startStagePoint = getElementPoint(stageElement, event.clientX, event.clientY)
  const gesture = {
    additive,
    baseIds,
    hasMoved: false,
    previewIds: baseIds,
    startScenePoint: getScenePoint(context, event),
    startStagePoint,
    mouseMove: handleWindowMouseMove.bind(null, context),
    mouseUp: handleWindowMouseUp.bind(null, context),
  }
  marqueeRef.current = gesture
  setSelectionRect({ ...startStagePoint, width: 0, height: 0 })
  window.addEventListener('mousemove', gesture.mouseMove)
  window.addEventListener('mouseup', gesture.mouseUp)
}

function setupMarqueeCleanup(marqueeRef) {
  return stopMarquee.bind(null, marqueeRef)
}

function selectWidgetSelection(state) {
  return { selectedWidgetId: state.selectedWidgetId, selectedWidgetIds: state.selectedWidgetIds, setWidgetSelection: state.setWidgetSelection }
}

function commitSelection(setWidgetSelection, orderedWidgetIds, widgetIds, preferredId = null) {
  const ids = normalizeSelectionIds(widgetIds, orderedWidgetIds)
  setWidgetSelection(ids, getPrimarySelectionId(ids, preferredId))
}

function getSelectedWidgets(widgetMap, widgetIds) {
  const widgets = []
  for (const id of widgetIds) widgets.push(widgetMap[id])
  return widgets
}

function getSelectedTargets(widgetNodes, widgetIds, isGroupSelection) {
  if (!isGroupSelection) return []
  const targets = []
  for (const id of widgetIds) {
    if (widgetNodes[id]) targets.push(widgetNodes[id])
  }
  return targets
}

function getElementGuidelines(widgetNodes, orderedWidgetIds, selectedWidgetIds) {
  const guidelines = []
  for (const id of orderedWidgetIds) {
    if (!selectedWidgetIds.includes(id) && widgetNodes[id]) guidelines.push(widgetNodes[id])
  }
  return guidelines
}

/** @param {object} options Widget order/map, mounted nodes, and group-drag selection. @returns {object} Selection presentation and actions. */
export default function useWidgetSelection({ displayScale, orderedWidgetIds, widgetMap, widgetNodes, moveableRef, sceneElement, sceneSize }) {
  const [hoveredWidgetId, setHoveredWidgetId] = useState(null)
  const [widgetBadges, setWidgetBadges] = useState([])
  const [groupGestureType, setGroupGestureType] = useState(null)
  const [groupDragSelectionIds, setGroupDragSelectionIds] = useState([])
  const [stageElement, setStageElement] = useState(null)
  const [selectionRect, setSelectionRect] = useState(null)
  const marqueeRef = useRef(null)
  useEffect(() => setupMarqueeCleanup(marqueeRef), [])
  const { selectedWidgetId, selectedWidgetIds, setWidgetSelection } = useStore(useShallow(selectWidgetSelection))
  const isGroupDragActive = groupGestureType === 'drag'
  // Store selection can advance before controlled config props after paste or deletion.
  const effectiveIds = useMemo(
    () => normalizeSelectionIds(isGroupDragActive ? groupDragSelectionIds : selectedWidgetIds, orderedWidgetIds),
    [groupDragSelectionIds, isGroupDragActive, orderedWidgetIds, selectedWidgetIds],
  )
  const areWidgetBadgesHidden = groupGestureType !== null
  const shouldRefreshBadges = useEffectEvent(
    (event) => !areWidgetBadgesHidden && (event.target.dataset.widgetId === hoveredWidgetId || effectiveIds.includes(event.target.dataset.widgetId)),
  )
  const refreshBadges = useEffectEvent(() => {
    if (areWidgetBadgesHidden) return
    const next = buildWidgetBadges({
      displayScale,
      getGeometry: (node) => getWidgetInteraction(node).geometry(),
      hoveredWidgetId,
      orderedWidgetIds,
      sceneElement,
      selectedWidgetIds: effectiveIds,
      widgetMap,
      widgetNodes,
    })
    setWidgetBadges((current) => (areWidgetBadgesEqual(current, next) ? current : next))
  })
  useLayoutEffect(() => refreshBadges(), [areWidgetBadgesHidden, displayScale, effectiveIds, hoveredWidgetId, sceneElement, widgetNodes])
  useEffect(() => {
    if (sceneElement === null) return
    let pending = false
    let active = true
    const rendered = (event) => {
      if (pending || !shouldRefreshBadges(event)) return
      pending = true
      queueMicrotask(() => {
        pending = false
        if (active) refreshBadges()
      })
    }
    sceneElement.addEventListener('wasm-widget-render', rendered)
    return () => {
      active = false
      sceneElement.removeEventListener('wasm-widget-render', rendered)
    }
  }, [sceneElement])
  const canvasMouseOver = useCallback((event) => setHoveredWidgetFromEvent(setHoveredWidgetId, event), [])
  const canvasMouseLeave = useCallback(() => setHoveredWidgetId(null), [])
  const selectedWidgets = useMemo(() => getSelectedWidgets(widgetMap, effectiveIds), [effectiveIds, widgetMap])
  const primaryId = getPrimarySelectionId(effectiveIds, selectedWidgetId)
  const isGroupSelection = effectiveIds.length > 1
  const selectedTargets = useMemo(
    () => getSelectedTargets(widgetNodes, effectiveIds, isGroupSelection),
    [effectiveIds, isGroupSelection, widgetNodes],
  )
  const elementGuidelines = useMemo(
    () => getElementGuidelines(widgetNodes, orderedWidgetIds, effectiveIds),
    [effectiveIds, orderedWidgetIds, widgetNodes],
  )
  const commit = useCallback(
    (widgetIds, preferredId) => commitSelection(setWidgetSelection, orderedWidgetIds, widgetIds, preferredId),
    [orderedWidgetIds, setWidgetSelection],
  )
  const pointerContext = useMemo(
    () => ({
      commitSelection: commit,
      marqueeRef,
      moveableRef,
      orderedWidgetIds,
      sceneElement,
      sceneSize,
      stageElement,
      selectedWidgetId,
      selectedWidgetIds,
      setGroupDragSelectionIds,
      setGroupGestureType,
      setSelectionRect,
      widgetNodes,
    }),
    [commit, moveableRef, orderedWidgetIds, sceneElement, sceneSize, stageElement, selectedWidgetId, selectedWidgetIds, widgetNodes],
  )
  const widgetMouseDown = useCallback((event, widgetId) => handleWidgetMouseDown(pointerContext, event, widgetId), [pointerContext])
  const sceneMouseDown = useCallback((event) => handleSceneMouseDown(pointerContext, event), [pointerContext])
  return {
    handleCanvasMouseOver: canvasMouseOver,
    handleCanvasMouseLeave: canvasMouseLeave,
    widgetBadges: areWidgetBadgesHidden ? [] : widgetBadges,
    handleSceneMouseDown: sceneMouseDown,
    handleWidgetMouseDown: widgetMouseDown,
    isGroupDragActive,
    selectionRect,
    setGroupDragSelectionIds,
    setGroupGestureType,
    setStageElement,
    elementGuidelines,
    isGroupSelection,
    selectedTarget: !isGroupSelection && primaryId !== null ? (widgetNodes[primaryId] ?? null) : null,
    selectedTargets,
    selectedWidget: primaryId === null ? null : widgetMap[primaryId],
    selectedWidgetId,
    selectedWidgetIds,
    selectedWidgets,
    setWidgetSelection,
  }
}
