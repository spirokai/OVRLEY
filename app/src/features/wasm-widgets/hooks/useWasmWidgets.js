import { useCallback, useEffect, useRef, useState } from 'react'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/preview-perf'
import useStore from '@/store/useStore'
import { createPreviewSession, loadWidgetModule } from '../utils/previewSession'

/**
 * @param {object} props Effective scene/widgets and the timeline second of activity second 0. The canonical playhead is
 * read from the store outside React so playback frames reach the session without re-rendering the editor.
 * @returns {object} Canvas binding and visible resource/render error.
 */
export function useWasmWidgets({ widgets, scene, activity, activityOriginSecond, disabledLaps, defaultFont }) {
  const session = useRef(null)
  const canvases = useRef(new Map())
  const latest = useRef(null)
  const originSecond = useRef(activityOriginSecond)
  const [error, setError] = useState(null)
  useEffect(() => {
    originSecond.current = activityOriginSecond
    latest.current = {
      widgets,
      scene,
      activityId: activity === null ? null : activity.preview_activity_id,
      disabledLaps,
      second: useStore.getState().selectedSecond - activityOriginSecond,
      defaultFont,
    }
    session.current?.setSnapshot(latest.current)
  }, [activity, activityOriginSecond, disabledLaps, widgets, scene, defaultFont])
  useEffect(
    () =>
      useStore.subscribe((state, previous) => {
        if (state.selectedSecond === previous.selectedSecond) return
        incrementPreviewPerfCounter(previewPerfCounterName('React preview updates'))
        latest.current = { ...latest.current, second: state.selectedSecond - originSecond.current }
        session.current?.setSnapshot(latest.current)
      }),
    [],
  )
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
