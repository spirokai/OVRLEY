import { useEffect, useEffectEvent, useRef } from 'react'
import { flushSync } from 'react-dom'
import { getWidgetInteraction } from '../utils/widgetInteraction'
import { dragTranslation, resizeAnchor } from '../utils/widgetGestureGeometry'

/**
 * Moveable owns pointer coordinates; WASM owns live configuration and tight geometry.
 * No legacy widget layout or scale policy is used here.
 * @param {object} options Selection, draft lifecycle, commits and Moveable reference.
 * @returns {object} Single-widget drag/resize and mixed-selection drag handlers.
 */
export function useWasmWidgetInteractions({ selectedWidget, selectedWidgets, state, setIsGroupDragActive, setGroupDragSelectionIds }) {
  const gesture = useRef(null)

  const clear = () => {
    const current = gesture.current
    gesture.current = null
    if (!current) return
    state.clearWidgetDrafts(current.ids)
    state.endWidgetInteraction(current.ids[0])
    setIsGroupDragActive(false)
    setGroupDragSelectionIds([])
  }
  const cancel = useEffectEvent(() => {
    if (!gesture.current) return
    gesture.current.controller?.cancel()
    clear()
    state.moveableRef.current?.stopDrag()
  })
  const refreshGeometry = useEffectEvent((event) => {
    if (!selectedWidgets.some((widget) => widget.id === event.target.dataset.widgetId)) return
    // Commit the outline before the browser paints the newly presented WASM pixels and bounds.
    flushSync(() => state.moveableRef.current?.updateRect())
  })
  useEffect(() => {
    const scene = state.sceneElement
    if (!scene) return
    const rendered = (event) => refreshGeometry(event)
    scene.addEventListener('wasm-widget-render', rendered)
    return () => scene.removeEventListener('wasm-widget-render', rendered)
  }, [state.sceneElement])
  const selectionIds = selectedWidgets.map((widget) => widget.id).join('|')
  useEffect(() => {
    cancel()
  }, [state.config, selectionIds])
  useEffect(() => {
    const keydown = (event) => {
      if (event.key !== 'Escape' || !gesture.current) return
      event.preventDefault()
      event.stopPropagation()
      cancel()
    }
    const blur = () => cancel()
    window.addEventListener('keydown', keydown, true)
    window.addEventListener('blur', blur)
    return () => {
      window.removeEventListener('keydown', keydown, true)
      window.removeEventListener('blur', blur)
      cancel()
    }
  }, [])

  const begin = (target, type) => {
    const controller = getWidgetInteraction(target)
    const origin = controller.begin((patch) => state.setLiveWidgetDraft(selectedWidget.id, patch, null))
    state.beginWidgetInteraction(selectedWidget.id, type)
    gesture.current = { controller, origin, ids: [selectedWidget.id], type, axis: null }
    return gesture.current
  }
  const finish = () => {
    const current = gesture.current
    if (!current) return
    try {
      const patch = current.controller.finish()
      if (patch) state.commitWidgetUpdate(current.ids[0], patch)
    } finally {
      current.controller.cancel()
      clear()
    }
  }
  return {
    onDragStart: ({ target, set }) => {
      set([0, 0])
      begin(target, 'wasm-drag')
    },
    onDrag: ({ beforeTranslate, inputEvent }) => {
      const current = gesture.current
      if (!current) return
      current.controller.update({ type: 'drag', translate: dragTranslation(current, beforeTranslate, inputEvent.ctrlKey) })
    },
    onDragEnd: finish,
    onResizeStart: ({ target, direction, set, dragStart }) => {
      const current = begin(target, 'wasm-resize')
      const { bounds } = current.origin
      current.anchor = resizeAnchor(bounds, direction, selectedWidget.type === 'raster' ? current.origin.data.rotation : 0)
      set([bounds.width, bounds.height])
      if (dragStart) dragStart.set([0, 0])
    },
    onResize: ({ width, height }) => {
      const current = gesture.current
      if (!current) return
      current.controller.update({
        type: 'resize',
        geometry: {
          ...(selectedWidget.type === 'backdrop' || selectedWidget.type === 'raster'
            ? { size: [Math.max(width, 0.01), Math.max(height, 0.01)] }
            : { factor: Math.max(width / current.origin.bounds.width, 0.01) }),
          ...current.anchor,
        },
      })
    },
    onResizeEnd: finish,
    onRotateStart: ({ target, set }) => {
      const current = begin(target, 'wasm-rotate')
      set(current.origin.data.rotation)
    },
    onRotate: ({ beforeRotate, drag }) => {
      const current = gesture.current
      if (!current) return
      current.controller.update({ type: 'rotate', rotation: beforeRotate, translate: drag.beforeTranslate })
    },
    onRotateEnd: finish,
    onDragGroupStart: ({ events }) => {
      const ids = selectedWidgets.map((widget) => widget.id)
      state.beginWidgetInteraction(ids[0], 'wasm-group-drag')
      gesture.current = { ids, widgets: selectedWidgets, axis: null, updates: null }
      setIsGroupDragActive(true)
      setGroupDragSelectionIds(ids)
      events.forEach((event) => event.set([0, 0]))
    },
    onDragGroup: ({ events, inputEvent }) => {
      const current = gesture.current
      if (!current) return
      const [x, y] = dragTranslation(current, events[0].beforeTranslate, inputEvent.ctrlKey)
      current.updates = Object.fromEntries(current.widgets.map((widget) => [widget.id, { x: widget.data.x + x, y: widget.data.y + y }]))
      state.setLiveWidgetDraftsBatch(Object.fromEntries(Object.entries(current.updates).map(([id, data]) => [id, { data, layout: null }])))
    },
    onDragGroupEnd: () => {
      const current = gesture.current
      if (!current) return
      if (current.updates) state.commitWidgetUpdates(current.updates)
      clear()
    },
  }
}
