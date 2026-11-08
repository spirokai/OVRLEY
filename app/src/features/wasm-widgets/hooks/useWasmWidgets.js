import { useCallback, useEffect, useRef, useState } from 'react'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/preview-perf'
import useStore from '@/store/useStore'
import { createPreviewSession, loadWidgetModule } from '../utils/previewSession'

/**
 * @param {object} props Effective scene/widgets. The canonical playhead is read from the store outside React so playback
 * frames reach the session without re-rendering the editor.
 * @returns {object} Canvas binding and visible resource/render error.
 */
export function useWasmWidgets({ widgets, scene, activity, disabledLaps, defaultFont }) {
  const session = useRef(null)
  const canvases = useRef(new Map())
  const mapLayers = useRef(new Map())
  const latest = useRef(null)
  const [error, setError] = useState(null)
  useEffect(() => {
    latest.current = {
      widgets,
      scene,
      activityId: activity === null ? null : activity.preview_activity_id,
      disabledLaps,
      second: useStore.getState().selectedSecond,
      defaultFont,
    }
    session.current?.setSnapshot(latest.current)
  }, [activity, disabledLaps, widgets, scene, defaultFont])
  useEffect(
    () =>
      useStore.subscribe((state, previous) => {
        if (state.selectedSecond === previous.selectedSecond) return
        incrementPreviewPerfCounter(previewPerfCounterName('React preview updates'))
        latest.current = { ...latest.current, second: state.selectedSecond }
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
        for (const [id, holder] of mapLayers.current) current.attachMapLayer(id, holder)
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
  const attachMapLayer = useCallback((id, holder) => {
    if (holder) mapLayers.current.set(id, holder)
    else mapLayers.current.delete(id)
    session.current?.attachMapLayer(id, holder)
  }, [])
  return { attach, attachMapLayer, error }
}
