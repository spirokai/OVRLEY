/**
 * Pointer and keyboard interaction for the linear gradient picker.
 */

import { useEffect, useRef } from 'react'
import { useAsRef } from '@/hooks/useRefs'
import {
  MAX_GRADIENT_STOPS,
  MIN_GRADIENT_STOPS,
  addGradientStop,
  moveGradientStop,
  removeGradientStop,
  setGradientStopColor,
  toGradientOffset,
} from '@/lib/gradient-stops'

const DRAG_THRESHOLD_PX = 3
const KEYBOARD_STEP = 0.01
const KEYBOARD_LARGE_STEP = 0.1

/**
 * Manages adding, dragging, recoloring and removing gradient stops.
 *
 * @param {object} options - Hook options.
 * @param {{ offset: number, color: string }[]} options.stops - Gradient stops.
 * @param {(stops: { offset: number, color: string }[]) => void} options.onChange - Receives the next stops, once per frame while dragging.
 * @param {() => void} [options.onCommit] - Called once an interaction ends, for consumers that treat `onChange` as a live preview.
 * @returns {object} Track ref, track handlers, and per-stop handler factories.
 */
export function useGradientPicker({ stops, onChange, onCommit }) {
  const trackRef = useRef(null)
  const dragRef = useRef(null)
  const suppressClickRef = useRef(false)
  const frameRef = useRef(null)
  const latestRef = useAsRef({ stops, onChange })

  // Discrete edits are complete interactions: preview and commit together.
  const applyStops = (nextStops) => {
    onChange(nextStops)
    onCommit?.()
  }

  useEffect(() => () => cancelAnimationFrame(frameRef.current), [])

  // Pointer moves fire far faster than the display refreshes; apply at most one move per frame.
  const flushDrag = () => {
    cancelAnimationFrame(frameRef.current)
    frameRef.current = null
    const drag = dragRef.current
    if (drag?.pendingOffset === undefined) return
    const { stops: latestStops, onChange: latestOnChange } = latestRef.current
    latestOnChange(moveGradientStop(latestStops, drag.index, drag.pendingOffset))
    drag.pendingOffset = undefined
  }

  const canAddStop = stops.length < MAX_GRADIENT_STOPS
  const canRemoveStop = stops.length > MIN_GRADIENT_STOPS

  const offsetFromClientX = (clientX) => {
    const rect = trackRef.current.getBoundingClientRect()
    return toGradientOffset((clientX - rect.left) / rect.width)
  }

  const removeStop = (index) => {
    if (canRemoveStop) {
      applyStops(removeGradientStop(stops, index))
    }
  }

  const handleTrackPointerDown = (event) => {
    // Pointer events from the portaled color popover bubble through the React tree.
    if (event.button !== 0 || !canAddStop || !event.currentTarget.contains(event.target)) return
    applyStops(addGradientStop(stops, offsetFromClientX(event.clientX)))
  }

  const getStopHandlers = (index) => ({
    onPointerDown: (event) => {
      if (event.button !== 0) return
      event.stopPropagation()
      event.currentTarget.setPointerCapture(event.pointerId)
      dragRef.current = { index, startX: event.clientX, moved: false, pendingOffset: undefined }
    },
    onPointerMove: (event) => {
      const drag = dragRef.current
      if (!drag) return
      if (!drag.moved && Math.abs(event.clientX - drag.startX) < DRAG_THRESHOLD_PX) return
      drag.moved = true
      drag.pendingOffset = offsetFromClientX(event.clientX)
      frameRef.current ??= requestAnimationFrame(flushDrag)
    },
    onPointerUp: (event) => {
      event.currentTarget.releasePointerCapture(event.pointerId)
      flushDrag()
      const moved = dragRef.current?.moved ?? false
      suppressClickRef.current = moved
      dragRef.current = null
      if (moved) onCommit?.()
    },
    onClick: (event) => {
      // Preventing default stops the popover trigger from toggling after a drag.
      if (suppressClickRef.current) {
        event.preventDefault()
        suppressClickRef.current = false
      }
    },
    onKeyDown: (event) => {
      if (event.key === 'Delete' || event.key === 'Backspace') {
        event.preventDefault()
        removeStop(index)
        return
      }
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        event.preventDefault()
        const step = (event.shiftKey ? KEYBOARD_LARGE_STEP : KEYBOARD_STEP) * (event.key === 'ArrowLeft' ? -1 : 1)
        applyStops(moveGradientStop(stops, index, toGradientOffset(stops[index].offset + step)))
      }
    },
  })

  const setStopColor = (index, color) => {
    applyStops(setGradientStopColor(stops, index, color))
  }

  return { trackRef, canAddStop, canRemoveStop, handleTrackPointerDown, getStopHandlers, setStopColor, removeStop }
}
