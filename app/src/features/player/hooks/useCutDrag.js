/**
 * Horizontal drag of cut markers: the cut follows the pointer frame by frame and is
 * committed once, on release, so the drag is one undo step.
 */

import { useCallback, useRef, useState } from 'react'
import { framesToSeconds, secondsToFrames } from '@/lib/video-program/frame-grid'
import { clampCutFrame } from '../utils/cutDrag'
import { viewPxToSeconds } from '../utils/timelineGeometry'

const DRAG_THRESHOLD_PX = 3

/**
 * Owns the dragged cut and the pointer handlers of cut markers. A press that does
 * not move past the threshold stays a click, which opens the marker's popover.
 *
 * @param {object} options Drag inputs.
 * @param {(videoId: string, cutId: string, frame: number) => void} options.moveCut Commits a cut's new frame.
 * @param {number} options.viewStart Visible viewport start second.
 * @param {number} options.viewEnd Visible viewport end second.
 * @param {number} options.widthPx Measured timeline width.
 * @returns {{draft: {videoId: string, cutId: string, frame: number}|null, getCutDragProps: function}} Dragged cut and marker props.
 */
export default function useCutDrag({ moveCut, viewStart, viewEnd, widthPx }) {
  const [draft, setDraft] = useState(null)
  const gestureRef = useRef(null)
  const suppressClickRef = useRef(false)

  const getCutDragProps = useCallback(
    // `clip` already shows the draft while dragging, so the gesture keeps the cut's starting frame.
    (clip, cut) => ({
      onPointerDown: (event) => {
        event.stopPropagation()
        suppressClickRef.current = false
        if (event.button !== 0) return
        event.currentTarget.setPointerCapture(event.pointerId)
        gestureRef.current = { pointerId: event.pointerId, startX: event.clientX, startFrame: cut.frame, frame: cut.frame, isMoving: false }
      },
      onPointerMove: (event) => {
        const gesture = gestureRef.current
        if (gesture === null || gesture.pointerId !== event.pointerId) return
        const deltaPx = event.clientX - gesture.startX
        if (!gesture.isMoving && Math.abs(deltaPx) < DRAG_THRESHOLD_PX) return
        gesture.isMoving = true
        const deltaSeconds = viewPxToSeconds({ deltaPx, viewStart, viewEnd, widthPx })
        const frame = clampCutFrame(clip, cut.id, secondsToFrames(framesToSeconds(gesture.startFrame, clip.fps) + deltaSeconds, clip.fps))
        if (frame === gesture.frame) return
        gesture.frame = frame
        setDraft({ videoId: clip.id, cutId: cut.id, frame })
      },
      onPointerUp: (event) => {
        const gesture = gestureRef.current
        if (gesture === null || gesture.pointerId !== event.pointerId) return
        gestureRef.current = null
        if (!gesture.isMoving) return
        suppressClickRef.current = true
        setDraft(null)
        if (gesture.frame !== gesture.startFrame) moveCut(clip.id, cut.id, gesture.frame)
      },
      onPointerCancel: () => {
        gestureRef.current = null
        setDraft(null)
      },
      // The click that ends a drag must not open the popover.
      onClick: (event) => {
        if (!suppressClickRef.current) return
        suppressClickRef.current = false
        event.preventDefault()
      },
    }),
    [moveCut, viewEnd, viewStart, widthPx],
  )

  return { draft, getCutDragProps }
}
