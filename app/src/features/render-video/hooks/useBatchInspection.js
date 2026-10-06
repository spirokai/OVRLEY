import { useEffect, useMemo, useState } from 'react'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'
import { useBatchRenderStore, useBatchSyncInputs } from '@/hooks/useAppStoreSelectors'
import { openDirectoryPath } from '@/lib/file-dialog'
import { captureRenderEncoding } from '../utils/renderRequest'
import { reviewBatchQueue, reviewBatchSync } from '../utils/renderPresentation'

/**
 * Owns disposable native source inspection and output review while confirmation is open.
 * Context identities prevent late results from authorizing Start.
 * @param {object} options Dialog phase and canonical settings.
 * @returns {object} Reviewed submission inputs and folder/queue controls. Execution and progress have separate owners.
 */
export default function useBatchInspection({ phase, settings }) {
  const store = useBatchRenderStore()
  const inputs = useBatchSyncInputs()
  const open = phase === 'confirm' && settings?.renderTarget === 'batch' && !store.hasBatchResults
  const folder = store.batchVideoFolder
  const outputDirectory = store.batchOutputFolder
  const choices = store.batchQueue
  const sync = useMemo(() => reviewBatchSync(inputs), [inputs])
  const [revision, setRevision] = useState(0)
  const [inspection, setInspection] = useState(null)
  const [plan, setPlan] = useState(null)
  const referencePath = sync.error ? null : (sync.calibration.reference?.path ?? null)
  const context = useMemo(() => ({ folder, referencePath, revision, open }), [folder, referencePath, revision, open])
  const current = inspection?.context === context ? inspection : null
  const rows = useMemo(() => reviewBatchQueue(choices, current, sync), [choices, current, sync])
  const jobs = useMemo(() => rows.filter((row) => row.status === 'pending'), [rows])
  const review = useMemo(
    () => ({ current, jobs, sync, outputDirectory, encoding: settings ? captureRenderEncoding(settings, inputs.availableCodecs) : null }),
    [current, jobs, sync, outputDirectory, settings, inputs.availableCodecs],
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
  const rowsWithIssues = rows.map((row) => (staleIds.has(row.source?.sourceId) ? { ...row, status: 'blocked', error: 'reinspectionRequired' } : row))
  const ready = open && currentPlan?.status === 'planned'
  const batchRunning = store.batchRunning || store.batchSubmissionPending
  const pickVideoFolder = async () => {
    const directory = await openDirectoryPath({ lastDirectoryKey: 'last-batch-video-dir' })
    if (directory !== null) store.setBatchVideoFolder(directory)
  }
  const pickOutputFolder = async () => {
    const directory = await openDirectoryPath({ lastDirectoryKey: 'last-batch-output-dir' })
    if (directory !== null) store.setBatchOutputFolder(directory)
  }
  return {
    ...store,
    batchRunning,
    batchQueue: rowsWithIssues,
    batchReady: ready && !batchRunning,
    batchReviewError: current?.error ?? sync.error ?? currentPlan?.error ?? null,
    request: ready
      ? { inspectionId: current.inspectionId, outputDirectory, jobs, calibrationSource: current.calibrationSource, sync, reviewedInputs: inputs }
      : null,
    pickVideoFolder,
    pickOutputFolder,
    refreshInspection: () => setRevision((value) => value + 1),
    reject: (error) => setInspection({ ...current, error: 'reinspectionRequired', issues: error.issues }),
  }
}
