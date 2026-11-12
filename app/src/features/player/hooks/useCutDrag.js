/**
 * Horizontal drag of cut markers, including a clip's edge cuts: the cut follows the
 * pointer frame by frame and is committed once, on release, so the drag is one undo
 * step. Releasing an edge cut trims that end of the video. Pressing a cut keeps its
 * clip selected, or selects the segment it belongs to.
 */

import { useCallback, useRef, useState } from 'react'
import { framesToSeconds, secondsToFrames } from '@/lib/video-program/frame-grid'
import { clampCutFrame, edgeCutSide } from '../utils/cutDrag'
import { viewPxToSeconds } from '../utils/timelineGeometry'

const DRAG_THRESHOLD_PX = 3

/**
 * Owns the dragged cut and the pointer handlers of cut markers. A press that does
 * not move past the threshold stays a click, which opens a cut marker's popover.
 *
 * @param {object} options Drag inputs.
 * @param {(videoId: string, cutId: string, frame: number) => void} options.moveCut Commits a cut's new frame.
 * @param {(videoId: string, side: 'start'|'end', frame: number) => void} options.trimVideo Commits a dragged edge cut.
 * @param {(clip: object, cut: {id: string, frame: number}) => void} options.selectCutClip Keeps or selects the pressed cut's clip.
 * @param {number} options.viewStart Visible viewport start second.
 * @param {number} options.viewEnd Visible viewport end second.
 * @param {number} options.widthPx Measured timeline width.
 * @returns {{draft: {videoId: string, cutId: string, frame: number}|null, getCutDragProps: function}} Dragged cut and marker props.
 */
export default function useCutDrag({ moveCut, selectCutClip, trimVideo, viewStart, viewEnd, widthPx }) {
  const [draft, setDraft] = useState(null)
  const gestureRef = useRef(null)
  const suppressClickRef = useRef(false)

  const getCutDragProps = useCallback(
    // `clip` already shows the draft while dragging, so the gesture keeps the cut's starting frame.
    (clip, cut) => {
      const resolve = (frame) => {
        const legalFrame = clampCutFrame(clip, cut.id, frame)
        return legalFrame === null ? null : { videoId: clip.id, cutId: cut.id, frame: legalFrame }
      }
      return {
        onPointerDown: (event) => {
          event.stopPropagation()
          suppressClickRef.current = false
          if (event.button !== 0) return
          selectCutClip(clip, cut)
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
          const next = resolve(secondsToFrames(framesToSeconds(gesture.startFrame, clip.fps) + deltaSeconds, clip.fps))
          const frame = next?.frame ?? gesture.startFrame
          if (frame === gesture.frame) return
          gesture.frame = frame
          setDraft(next)
        },
        onPointerUp: (event) => {
          const gesture = gestureRef.current
          if (gesture === null || gesture.pointerId !== event.pointerId) return
          gestureRef.current = null
          if (!gesture.isMoving) return
          suppressClickRef.current = true
          setDraft(null)
          if (gesture.frame === gesture.startFrame) return
          const side = edgeCutSide(cut.id)
          if (side === null) moveCut(clip.id, cut.id, gesture.frame)
          else trimVideo(clip.id, side, gesture.frame)
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
      }
    },
    [moveCut, selectCutClip, trimVideo, viewEnd, viewStart, widthPx],
  )

  return { draft, getCutDragProps }
}
