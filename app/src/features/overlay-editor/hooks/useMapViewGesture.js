/**
 * The map's view mode: dragging pans a top-down map and the wheel zooms it. Rust turns each gesture
 * into a patch. Patches preview as drafts and commit when the drag ends, the wheel settles, or the
 * mode ends.
 */

import { useEffect, useEffectEvent, useRef } from 'react'
import { hasOpenOverlay, matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { getWidgetInteraction } from '@/features/wasm-widgets/utils/widgetInteraction'
import { getElementPoint } from '../utils/selectionGeometry'

const WHEEL_COMMIT_DELAY_MS = 300

/**
 * @param {object} options
 * @param {object|null} options.widget - The map in view mode, or null outside it.
 * @param {HTMLElement|undefined} options.node - The map's wrapper.
 * @param {HTMLElement|null} options.sceneElement - The scene the map is drawn in.
 * @param {object} options.widgetMap - Effective widgets by ID.
 * @param {Function} options.setLiveWidgetDraft - Previews a patch.
 * @param {Function} options.commitWidgetUpdate - Commits a patch.
 * @param {Function} options.setMapViewWidgetId - Enters or leaves a map's view mode.
 */
export function useMapViewGesture({ widget, node, sceneElement, widgetMap, setLiveWidgetDraft, commitWidgetUpdate, setMapViewWidgetId }) {
  const id = widget === null ? null : widget.id
  const canPan = widget !== null && widget.data.camera_mode === 'fixed'
  // The uncommitted patch, with the data it applies to, since a wheel tick can arrive before the draft renders.
  const pending = useRef(null)
  const stopDrag = useRef(null)

  const scenePoint = useEffectEvent((event) => {
    const point = getElementPoint(sceneElement, event.clientX, event.clientY)
    return [point.x, point.y]
  })
  const exit = useEffectEvent(() => setMapViewWidgetId(null))
  const current = useEffectEvent(() => (pending.current === null ? widget.data : pending.current.data))
  const preview = useEffectEvent((data, gesture) => {
    const patch = getWidgetInteraction(node).resize({ widget: data, map_view: gesture })
    const before = pending.current ?? { data: widget.data, patch: {}, timer: null }
    pending.current = { ...before, data: { ...before.data, ...patch }, patch: { ...before.patch, ...patch } }
    setLiveWidgetDraft(widget.id, patch)
  })
  const commit = useEffectEvent((widgetId) => {
    const edit = pending.current
    pending.current = null
    if (edit === null) return
    clearTimeout(edit.timer)
    // Deleting the map also ends the mode, and leaves nothing to commit to.
    if (widgetMap[widgetId]) commitWidgetUpdate(widgetId, edit.patch)
  })
  const zoom = useEffectEvent((event) => {
    preview(current(), { zoom: { wheel_delta: event.deltaY, point: scenePoint(event) } })
    clearTimeout(pending.current.timer)
    pending.current.timer = setTimeout(() => commit(widget.id), WHEEL_COMMIT_DELAY_MS)
  })
  const startDrag = useEffectEvent((event) => {
    const start = current()
    const origin = scenePoint(event)
    if (pending.current !== null) clearTimeout(pending.current.timer)
    const move = (moveEvent) => {
      const point = scenePoint(moveEvent)
      preview(start, { pan: [point[0] - origin[0], point[1] - origin[1]] })
    }
    const up = () => {
      stopDrag.current()
      commit(widget.id)
    }
    stopDrag.current = () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
      stopDrag.current = null
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  })

  useEffect(() => {
    if (id === null || !node) return
    const mouseDown = (event) => {
      if (event.button !== 0) return
      // The map keeps its selection: nothing else may start a move or a marquee.
      event.preventDefault()
      event.stopPropagation()
      if (canPan) startDrag(event)
    }
    const wheel = (event) => {
      // Modified wheels keep zooming and scrolling the canvas.
      if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return
      event.preventDefault()
      event.stopPropagation()
      zoom(event)
    }
    // Clicking the canvas keeps focus where it was, often in the sidebar, so Escape leaves the mode
    // wherever focus is. It stays with a dialog or popup that's open.
    const keyDown = (event) => {
      if (matchKeyboardShortcut(event, 'editor')?.commandId !== 'editor.clearSelection' || hasOpenOverlay()) return
      event.preventDefault()
      event.stopPropagation()
      exit()
    }
    node.addEventListener('mousedown', mouseDown)
    node.addEventListener('wheel', wheel, { passive: false })
    window.addEventListener('keydown', keyDown, true)
    return () => {
      node.removeEventListener('mousedown', mouseDown)
      node.removeEventListener('wheel', wheel)
      window.removeEventListener('keydown', keyDown, true)
      if (stopDrag.current !== null) stopDrag.current()
      commit(id)
    }
  }, [id, node, canPan])
}
