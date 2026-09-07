import { useEffect, useMemo, useRef, useState } from 'react'
import { useVisualSyncStore } from '@/hooks/useAppStoreSelectors'
import useStore from '@/store/useStore'
import * as backend from '@/api/backend'
import { describeVisualSyncCandidate, formatAnalyzedSeconds, sameVisualSyncInputs, visualSyncStatus } from '../utils/visualSyncUtils'
import { useTranslation } from 'react-i18next'

const activityRevisions = new WeakMap()

function activityRevision(activity) {
  if (!activityRevisions.has(activity)) activityRevisions.set(activity, crypto.randomUUID())
  return activityRevisions.get(activity)
}

function currentInputs(session) {
  const state = useStore.getState()
  return (
    state.parsedActivity === session.activity &&
    state.importedVideoImportId === session.inputs.video_revision &&
    state.importedVideoPath === session.path
  )
}

/** Owns analysis above drawer mount lifetime. @returns {object} Drawer state and explicit actions. */
export function useVisualSync() {
  const { t } = useTranslation()
  const source = useVisualSyncStore()
  const { parsedActivity, activitySource, parsedActivitySource, importedVideoPath, importedVideoImportId, setVideoSyncOffset } = source
  const inputs = useMemo(() => {
    if (!parsedActivity || !importedVideoPath || !importedVideoImportId) return null
    return {
      video_identity: importedVideoPath,
      video_revision: importedVideoImportId,
      activity_identity: activitySource ? activitySource.path : parsedActivitySource,
      activity_revision: activityRevision(parsedActivity),
    }
  }, [parsedActivity, activitySource, parsedActivitySource, importedVideoPath, importedVideoImportId])
  const active = useRef(null)
  const [view, setView] = useState({ status: 'idle', snapshot: null, error: null, applied: null })

  useEffect(() => {
    setView({ status: 'idle', snapshot: null, error: null, applied: null })
    return () => {
      const session = active.current
      if (!session) return
      active.current = null
      session.dispose()
      if (session.jobId) backend.cancelVisualSync(session.jobId).catch((error) => console.error('Visual sync cancellation failed', error))
    }
  }, [inputs])

  const start = async () => {
    if (!inputs || active.current) return
    const session = { inputs, activity: parsedActivity, path: importedVideoPath, jobId: null, snapshot: null, dispose: () => {}, timer: null }
    active.current = session
    const isCurrent = () => active.current === session && currentInputs(session)
    const release = () => {
      session.dispose()
      if (active.current === session) active.current = null
    }
    const fail = (error) => {
      if (!isCurrent()) return
      if (session.jobId) backend.cancelVisualSync(session.jobId).catch((cancelError) => console.error(cancelError))
      setView({ status: 'error', snapshot: null, error: error.message, applied: null })
      release()
    }
    const receive = (snapshot) => {
      if (!isCurrent() || !sameVisualSyncInputs(snapshot.inputs, inputs)) return
      if (session.jobId === null || snapshot.job_id !== session.jobId) return
      if (session.snapshot && snapshot.sequence <= session.snapshot.sequence) return
      session.snapshot = snapshot
      const terminal = snapshot.terminal
      const status = visualSyncStatus(snapshot)
      setView({ status, snapshot, error: terminal?.kind === 'error' ? terminal.message : null, applied: null })
      if (terminal !== null) release()
    }
    setView({ status: 'analyzing', snapshot: null, error: null, applied: null })
    try {
      const unsubscribe = await backend.subscribeVisualSync(receive)
      session.dispose = () => {
        unsubscribe()
        clearTimeout(session.timer)
      }
      if (!isCurrent()) {
        release()
        return
      }
      const initial = await backend.startVisualSync({
        inputs,
        video_path: importedVideoPath,
        parsed_activity_json: JSON.stringify(parsedActivity),
        analysis_settings: { frames_per_second: 20, long_edge_pixels: 640, start_seconds: 0, end_seconds: null },
      })
      session.jobId = initial.job_id
      if (!isCurrent()) {
        await backend.cancelVisualSync(initial.job_id)
        release()
        return
      }
      receive(initial)
      // Status recovers completion before start resolved, and dropped events.
      const reconcile = async () => {
        if (!isCurrent()) return
        try {
          receive(await backend.getVisualSyncStatus(initial.job_id))
          if (isCurrent()) session.timer = setTimeout(reconcile, 1000)
        } catch (error) {
          fail(error)
        }
      }
      await reconcile()
    } catch (error) {
      fail(error)
    }
  }

  const cancel = async () => {
    const session = active.current
    if (!session) return
    if (session.jobId === null) {
      active.current = null
      session.dispose()
      setView({ status: 'cancelled', snapshot: null, error: null, applied: null })
      return
    }
    try {
      await backend.cancelVisualSync(session.jobId)
    } catch (error) {
      if (active.current === session) setView((previous) => ({ ...previous, error: error.message }))
    }
  }

  const apply = (offsetSeconds) => {
    if (
      !inputs ||
      !view.snapshot ||
      !sameVisualSyncInputs(view.snapshot.inputs, inputs) ||
      !currentInputs({ activity: parsedActivity, path: importedVideoPath, inputs })
    )
      return
    const terminal = view.snapshot.terminal
    if (terminal?.kind !== 'result' || !terminal.result.accepted) return
    const candidate = terminal.result.candidates.find((item) => item.offset_seconds === offsetSeconds)
    if (!candidate?.accepted) return
    setVideoSyncOffset(candidate.offset_seconds)
    setView((previous) => ({ ...previous, applied: offsetSeconds }))
  }

  const terminal = view.snapshot?.terminal
  const result = terminal?.kind === 'result' ? terminal.result : null
  const candidates =
    result === null ? [] : result.candidates.map((candidate) => describeVisualSyncCandidate(candidate, view.applied === candidate.offset_seconds, t))
  const analyzedSeconds = view.snapshot?.analyzed_seconds
  return {
    ...view,
    ready: inputs !== null,
    busy: ['analyzing', 'matching'].includes(view.status),
    statusLabel: view.status === 'idle' || view.status === 'error' ? null : t(`syncDoctor.status.${view.status}`),
    progressLabel: analyzedSeconds == null ? null : t('syncDoctor.analyzedSeconds', { seconds: formatAnalyzedSeconds(analyzedSeconds) }),
    candidates,
    start,
    cancel,
    apply,
  }
}
