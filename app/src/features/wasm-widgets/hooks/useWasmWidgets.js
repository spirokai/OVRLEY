import { useCallback, useEffect, useRef, useState } from 'react'
import { createPreviewSession, loadWidgetModule } from '../utils/previewSession'

/** @param {object} props Effective scene/widgets and canonical playhead. @returns {object} Canvas binding and visible resource/render error. */
export function useWasmWidgets({ widgets, scene, activity, disabledLaps, second, defaultFont }) {
  const session = useRef(null)
  const canvases = useRef(new Map())
  const latest = useRef(null)
  const [error, setError] = useState(null)
  useEffect(() => {
    latest.current = { widgets, scene, activityId: activity === null ? null : activity.preview_activity_id, disabledLaps, second, defaultFont }
    session.current?.setSnapshot(latest.current)
  }, [activity, disabledLaps, widgets, scene, second, defaultFont])
  useEffect(() => {
    let cancelled = false
    loadWidgetModule()
      .then((module) => {
        if (cancelled) return
        const current = createPreviewSession(module, setError)
        session.current = current
        for (const [id, { canvas, target }] of canvases.current) current.attach(id, canvas, target)
        current.setSnapshot(latest.current)
      })
      .catch((cause) => {
        if (!cancelled) setError(cause)
      })
    return () => {
      cancelled = true
      session.current?.dispose()
      session.current = null
    }
  }, [])
  const attach = useCallback((id, canvas, target) => {
    if (canvas) canvases.current.set(id, { canvas, target })
    else canvases.current.delete(id)
    session.current?.attach(id, canvas, target)
  }, [])
  return { attach, error }
}
