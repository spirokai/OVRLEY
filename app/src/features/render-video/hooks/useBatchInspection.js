import { useEffect, useMemo, useState } from 'react'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'
import { captureBatchEncoding } from '../utils/batchRenderRequest'
import { reviewBatchQueue } from '../utils/batchRenderReview'

/**
 * Owns one disposable configuration session and its native output review.
 * Context identities prevent late results from authorizing Start.
 * @param {object} options Opening, folder, queue choices, sync and encoder inputs.
 * @returns {object} Current inspection, reviewed rows, readiness, error and refresh/rejection actions.
 */
export default function useBatchInspection({ open, folder, outputDirectory, choices, sync, settings, availableCodecs }) {
  const [revision, setRevision] = useState(0)
  const [inspection, setInspection] = useState(null)
  const [plan, setPlan] = useState(null)
  const referencePath = sync.error ? null : (sync.calibration.reference?.path ?? null)
  const context = useMemo(() => ({ open, folder, referencePath, revision }), [open, folder, referencePath, revision])
  const current = inspection?.context === context ? inspection : null
  const rows = useMemo(() => reviewBatchQueue(choices, current, sync), [choices, current, sync])
  const jobs = useMemo(() => rows.filter((row) => row.status === 'pending'), [rows])
  const review = useMemo(
    () => ({ current, jobs, sync, outputDirectory, encoding: settings ? captureBatchEncoding(settings, availableCodecs) : null }),
    [current, jobs, sync, outputDirectory, settings, availableCodecs],
  )

  useEffect(() => {
    if (!open || folder === null) return
    let closed = false
    let inspectionId = null
    void (async () => {
      try {
        const created = await backend.createVideoInspection()
        inspectionId = created.inspectionId
        if (closed) {
          await backend.disposeVideoInspection(inspectionId)
          return
        }
        const paths = await backend.listDirectoryVideoFiles(folder)
        if (closed) return
        useStore.getState().setBatchQueueFromPaths(paths)
        const sources = new Map()
        const inspectPaths = referencePath !== null && !paths.includes(referencePath) ? [...paths, referencePath] : paths
        // Requests are independent; Rust bounds active probes across all sessions.
        await Promise.all(
          inspectPaths.map(async (path) => {
            try {
              sources.set(path, { source: await backend.inspectVideoSource(inspectionId, path), error: null })
            } catch (error) {
              sources.set(path, { source: null, error: error.message })
            }
            if (!closed)
              setInspection({
                context,
                inspectionId,
                sources: new Map(sources),
                complete: false,
                calibrationSource: sources.get(referencePath)?.source ?? null,
                error: null,
              })
          }),
        )
        if (!closed)
          setInspection({
            context,
            inspectionId,
            sources,
            complete: true,
            calibrationSource: referencePath === null ? null : sources.get(referencePath).source,
            error: referencePath === null ? null : sources.get(referencePath).error,
          })
      } catch (error) {
        if (!closed) setInspection({ context, error: error.message, sources: new Map() })
      }
    })()
    return () => {
      closed = true
      if (inspectionId !== null)
        void backend.disposeVideoInspection(inspectionId).catch((error) => useStore.getState().setErrorMessage(error.message))
    }
  }, [open, folder, referencePath, context])

  useEffect(() => {
    if (!open || !current?.complete || current.error || sync.error || jobs.length === 0 || outputDirectory === null) return
    let closed = false
    void backend
      .planBatchOutputs(
        {
          inspectionId: current.inspectionId,
          sourceIds: jobs.map((row) => row.source.sourceId),
          calibrationSourceId: current.calibrationSource?.sourceId ?? null,
        },
        review.encoding,
        outputDirectory,
      )
      .then((result) => {
        if (closed) return
        if (result.status === 'rejected') setInspection({ ...current, error: 'reinspectionRequired', issues: result.issues })
        else setPlan({ review, ...result })
      })
      .catch((error) => {
        if (!closed) setPlan({ review, status: 'error', error: error.message })
      })
    return () => {
      closed = true
    }
  }, [open, current, sync, jobs, outputDirectory, review])

  const currentPlan = plan?.review === review ? plan : null
  const staleIds = new Set((current?.issues ?? []).map((issue) => issue.sourceId))
  return {
    inspection: current,
    rows: rows.map((row) => (staleIds.has(row.source?.sourceId) ? { ...row, status: 'blocked', error: 'reinspectionRequired' } : row)),
    jobs,
    ready: open && currentPlan?.status === 'planned',
    error: current?.error ?? sync.error ?? currentPlan?.error ?? null,
    refresh: () => setRevision((value) => value + 1),
    reject: (error) => setInspection({ ...current, error: 'reinspectionRequired', issues: error.issues }),
  }
}
