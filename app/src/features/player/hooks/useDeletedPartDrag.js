/**
 * Alt+drag on a video clip draws a deleted part over the dragged source range.
 */

import { useCallback, useRef, useState } from 'react'
import { isDeletableRange } from '@/lib/video-program/deleted-parts'

function clipSecondAtPointer(event, durationSeconds) {
  const rect = event.currentTarget.getBoundingClientRect()
  if (rect.width <= 0) return 0
  return Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width)) * durationSeconds
}

/**
 * Owns the transient range of an Alt+drag on a clip and commits it as a deleted
 * part on release. Handlers return whether they handled the event, so the clip's
 * move drag handles every other gesture.
 *
 * @param {{addDeletedPart: function}} options Store command adding a deleted part.
 * @returns {{preview: {videoId: string, fromSecond: number, toSecond: number}|null, onPointerDown: function, onPointerMove: function, onPointerUp: function, onPointerCancel: function}}
 *   Drag preview and pointer handlers.
 */
export default function useDeletedPartDrag({ addDeletedPart }) {
  const dragRef = useRef(null)
  const [preview, setPreview] = useState(null)

  const publish = useCallback(() => {
    const drag = dragRef.current
    setPreview({
      videoId: drag.clip.id,
      fromSecond: Math.min(drag.anchorSecond, drag.currentSecond),
      toSecond: Math.max(drag.anchorSecond, drag.currentSecond),
    })
  }, [])

  const onPointerDown = useCallback(
    (event, clip) => {
      if (!event.altKey || (event.button !== undefined && event.button !== 0)) return false
      event.stopPropagation()
      event.preventDefault()
      event.currentTarget.setPointerCapture?.(event.pointerId)
      const second = clipSecondAtPointer(event, clip.durationSeconds)
      dragRef.current = { clip, pointerId: event.pointerId, anchorSecond: second, currentSecond: second }
      publish()
      return true
    },
    [publish],
  )

  const onPointerMove = useCallback(
    (event) => {
      const drag = dragRef.current
      if (drag === null || drag.pointerId !== event.pointerId) return false
      event.stopPropagation()
      drag.currentSecond = clipSecondAtPointer(event, drag.clip.durationSeconds)
      publish()
      return true
    },
    [publish],
  )

  const finish = useCallback(
    (event, commit) => {
      const drag = dragRef.current
      if (drag === null || drag.pointerId !== event.pointerId) return false
      event.stopPropagation()
      event.currentTarget.releasePointerCapture?.(event.pointerId)
      dragRef.current = null
      setPreview(null)
      const fromSecond = Math.min(drag.anchorSecond, drag.currentSecond)
      const toSecond = Math.max(drag.anchorSecond, drag.currentSecond)
      if (commit && isDeletableRange(drag.clip, fromSecond, toSecond)) addDeletedPart(drag.clip.id, fromSecond, toSecond)
      return true
    },
    [addDeletedPart],
  )

  return {
    preview,
    onPointerDown,
    onPointerMove,
    onPointerUp: useCallback((event) => finish(event, true), [finish]),
    onPointerCancel: useCallback((event) => finish(event, false), [finish]),
  }
}
