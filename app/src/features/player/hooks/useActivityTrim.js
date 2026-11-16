/**
 * Trim of the external activity on the timeline: an edge handle of the activity
 * lane follows the pointer and the trim is committed once, on release, so the
 * drag is one undo step. Shortcuts set an edge at the playhead.
 */

import { useCallback, useRef, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { resolveActivityWindow, trimActivityEdge } from '@/lib/activity/activity-trim'
import useStore from '@/store/useStore'
import { viewPxToSeconds } from '../utils/timelineGeometry'

const DRAG_THRESHOLD_PX = 3

/**
 * @param {object} options Trim inputs.
 * @param {number} options.durationSeconds Activity duration.
 * @param {number} options.viewStart Visible viewport start second.
 * @param {number} options.viewEnd Visible viewport end second.
 * @param {number} options.widthPx Measured timeline width.
 * @returns {{trim: {startSecond: number, endSecond: number}|null, draggedEdge: 'start'|'end'|null, window: object, getEdgeDragProps: function, setEdge: function, reset: function}}
 *   Displayed trim (the draft while dragging), its window, edge handle props, and trim commands.
 */
export default function useActivityTrim({ durationSeconds, viewStart, viewEnd, widthPx }) {
  const { activityTrim, setActivityTrim } = useStore(
    useShallow((state) => ({ activityTrim: state.activityTrim, setActivityTrim: state.setActivityTrim })),
  )
  const [draft, setDraft] = useState(null)
  const gestureRef = useRef(null)
  const trim = draft === null ? activityTrim : draft.trim

  const setEdge = useCallback(
    (edge, second) => setActivityTrim(trimActivityEdge(activityTrim, durationSeconds, edge, second)),
    [activityTrim, durationSeconds, setActivityTrim],
  )
  const reset = useCallback(() => setActivityTrim(null), [setActivityTrim])

  const getEdgeDragProps = useCallback(
    (edge) => ({
      onPointerDown: (event) => {
        event.stopPropagation()
        if (event.button !== 0) return
        event.currentTarget.setPointerCapture(event.pointerId)
        const window = resolveActivityWindow(durationSeconds, activityTrim)
        gestureRef.current = {
          pointerId: event.pointerId,
          startX: event.clientX,
          startSecond: edge === 'start' ? window.startSecond : window.endSecond,
          trim: activityTrim,
          isMoving: false,
        }
      },
      onPointerMove: (event) => {
        const gesture = gestureRef.current
        if (gesture === null || gesture.pointerId !== event.pointerId) return
        const deltaPx = event.clientX - gesture.startX
        if (!gesture.isMoving && Math.abs(deltaPx) < DRAG_THRESHOLD_PX) return
        gesture.isMoving = true
        const deltaSeconds = viewPxToSeconds({ deltaPx, viewStart, viewEnd, widthPx })
        gesture.trim = trimActivityEdge(activityTrim, durationSeconds, edge, gesture.startSecond + deltaSeconds)
        setDraft({ edge, trim: gesture.trim })
      },
      onPointerUp: (event) => {
        const gesture = gestureRef.current
        if (gesture === null || gesture.pointerId !== event.pointerId) return
        gestureRef.current = null
        if (!gesture.isMoving) return
        setDraft(null)
        setActivityTrim(gesture.trim)
      },
      onPointerCancel: () => {
        gestureRef.current = null
        setDraft(null)
      },
    }),
    [activityTrim, durationSeconds, setActivityTrim, viewEnd, viewStart, widthPx],
  )

  return {
    trim,
    draggedEdge: draft?.edge ?? null,
    window: resolveActivityWindow(durationSeconds, trim),
    getEdgeDragProps,
    setEdge,
    reset,
  }
}
