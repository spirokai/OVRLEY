import { useEffect, useEffectEvent, useRef } from 'react'
import { flushSync } from 'react-dom'
import { getWidgetInteraction } from '../utils/widgetInteraction'
import { dragTranslation, groupResizeOrigin, resizeAnchor, widgetResizeGeometry } from '../utils/widgetGestureGeometry'

/**
 * Moveable owns pointer coordinates; WASM owns live configuration and tight geometry.
 * @param {object} options Selection, draft lifecycle, commits and Moveable reference.
 * @returns {object} Widget and group gesture handlers.
 */
export function useWasmWidgetInteractions({ selectedWidget, selectedWidgets, state, setGroupGestureType, setGroupDragSelectionIds }) {
  const gesture = useRef(null)

  const clear = () => {
    const current = gesture.current
    gesture.current = null
    if (!current) return
    state.clearWidgetDrafts(current.ids)
    setGroupGestureType(null)
    setGroupDragSelectionIds([])
  }
  const cancel = useEffectEvent(() => {
    if (!gesture.current) return
    gesture.current.controller.cancel()
    clear()
    state.moveableRef.current?.stopDrag()
  })
  const shouldRefreshGeometry = useEffectEvent((event) => selectedWidgets.some((widget) => widget.id === event.target.dataset.widgetId))
  useEffect(() => {
    const scene = state.sceneElement
    if (!scene) return
    let pending = false
    let active = true
    const rendered = (event) => {
      if (pending || !shouldRefreshGeometry(event)) return
      pending = true
      // Measure once after all widgets in the frame have updated, before the browser paints.
      queueMicrotask(() => {
        pending = false
        if (active) flushSync(() => state.moveableRef.current?.updateRect())
      })
    }
    scene.addEventListener('wasm-widget-render', rendered)
    return () => {
      active = false
      scene.removeEventListener('wasm-widget-render', rendered)
    }
  }, [state.sceneElement, state.moveableRef])
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

  const begin = (target) => {
    const controller = getWidgetInteraction(target)
    const origin = controller.begin((patch) => state.setLiveWidgetDraft(selectedWidget.id, patch))
    gesture.current = { controller, origin, ids: [selectedWidget.id], axis: null }
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
  const finishGroup = () => {
    const current = gesture.current
    if (!current) return
    try {
      const patches = current.controller.finish()
      if (patches) state.commitWidgetUpdates(patches)
    } finally {
      current.controller.cancel()
      clear()
    }
  }
  return {
    onDragStart: ({ target, set }) => {
      set([0, 0])
      begin(target)
    },
    onDrag: ({ beforeTranslate, inputEvent }) => {
      const current = gesture.current
      if (!current) return
      current.controller.update({ type: 'drag', translate: dragTranslation(current, beforeTranslate, inputEvent.ctrlKey) })
    },
    onDragEnd: finish,
    onResizeStart: ({ target, direction, set, dragStart }) => {
      const current = begin(target)
      const { bounds } = current.origin
      current.anchor = resizeAnchor(
        bounds,
        direction,
        ['course', 'elevation', 'diving_profile', 'raster'].includes(selectedWidget.type) ? current.origin.data.rotation : 0,
        current.origin.rotationOrigin,
      )
      set([bounds.width, bounds.height])
      if (dragStart) dragStart.set([0, 0])
    },
    onResize: ({ width, height }) => {
      const current = gesture.current
      if (!current) return
      current.controller.update({
        type: 'resize',
        geometry: {
          ...widgetResizeGeometry(selectedWidget, current.origin.bounds, width, height),
          ...current.anchor,
        },
      })
    },
    onResizeEnd: finish,
    onResizeGroupStart: ({ events, direction, moveable }) => {
      const ids = selectedWidgets.map((widget) => widget.id)
      const controller = getWidgetInteraction(events[0].target).group(ids)
      const origins = controller.begin(state.setLiveWidgetDrafts)
      gesture.current = {
        controller,
        origins,
        ids,
        widgets: selectedWidgets,
        direction,
        groupOrigin: groupResizeOrigin(moveable.getRect(), direction),
      }
      setGroupGestureType('resize')
      events.forEach(({ target, set, dragStart }) => {
        const { bounds } = origins[target.dataset.widgetId]
        set([bounds.width, bounds.height])
        if (dragStart) dragStart.set([0, 0])
      })
    },
    onResizeGroup: ({ boundingWidth }) => {
      const current = gesture.current
      if (!current) return
      // Child drag offsets depend on live DOM positions; resize everything from the captured group frame instead.
      const { width, point } = current.groupOrigin
      const factor = boundingWidth / width
      const intents = Object.fromEntries(
        current.widgets.map((widget) => {
          const origin = current.origins[widget.id]
          // Anchor each child at the group's fixed corner so the stationary group edges stay exact.
          const anchor = resizeAnchor(
            origin.bounds,
            current.direction,
            ['course', 'elevation', 'diving_profile', 'raster'].includes(widget.type) ? origin.data.rotation : 0,
            origin.rotationOrigin,
          )
          return [
            widget.id,
            {
              type: 'resize',
              geometry: {
                ...widgetResizeGeometry(widget, origin.bounds, origin.bounds.width * factor, origin.bounds.height * factor),
                fixed: anchor.fixed,
                point: anchor.point.map((value, index) => point[index] + (value - point[index]) * factor),
              },
            },
          ]
        }),
      )
      current.controller.update(intents)
    },
    onResizeGroupEnd: finishGroup,
    onRotateStart: ({ target, set }) => {
      const current = begin(target)
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
      const controller = getWidgetInteraction(events[0].target).group(ids)
      controller.begin(state.setLiveWidgetDrafts)
      gesture.current = { ids, controller, axis: null }
      setGroupGestureType('drag')
      setGroupDragSelectionIds(ids)
      events.forEach((event) => event.set([0, 0]))
    },
    onDragGroup: ({ events, inputEvent }) => {
      const current = gesture.current
      if (!current) return
      const translate = dragTranslation(current, events[0].beforeTranslate, inputEvent.ctrlKey)
      current.controller.update(Object.fromEntries(current.ids.map((id) => [id, { type: 'drag', translate }])))
    },
    onDragGroupEnd: finishGroup,
  }
}
