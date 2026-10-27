import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import * as backend from '@/api/backend'
import useStore from '@/store/useStore'
import { useBatchRenderStore, useBatchSyncInputs } from '@/hooks/useAppStoreSelectors'
import { openDirectoryPath } from '@/lib/file-dialog'
import { joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import { captureRenderEncoding, createExportDestination } from '../utils/renderRequest'
import { reviewExport } from '../utils/exportOutputs'
import { formatRenderPathError, reviewBatchSync } from '../utils/renderPresentation'

/**
 * Inspects every imported video for export, retains the inspection across dialog
 * openings, and plans the outputs of the selected output mode while confirmation
 * is open. Context identities prevent late results from authorizing Start.
 * @param {object} options Dialog phase and canonical settings.
 * @returns {object} Reviewed submission inputs, combined-export blockers and output-folder controls.
 *   Execution and progress have separate owners.
 */
export default function useBatchInspection({ phase, settings }) {
  const { i18n, t } = useTranslation()
  const store = useBatchRenderStore()
  const inputs = useBatchSyncInputs()
  const clips = useMemo(
    () => sortVideosByTimeline(joinVideoEdits(inputs.importedVideos, inputs.videoEdits)),
    [inputs.importedVideos, inputs.videoEdits],
  )
  const open = phase === 'confirm' && clips.length > 0 && !store.hasBatchResults
  const outputMode = settings?.outputMode ?? null
  const outputDirectory = store.batchOutputFolder
  const outputPath = settings?.outputPath ?? ''
  const sourcePaths = useMemo(() => clips.map((clip) => clip.path).join('\n'), [clips])
  const sync = useMemo(() => reviewBatchSync(inputs), [inputs])
  const [revision, setRevision] = useState(0)
  const [inspection, setInspection] = useState(null)
  const [plan, setPlan] = useState(null)
  const context = useMemo(() => ({ sourcePaths, revision }), [sourcePaths, revision])
  const current = inspection?.context === context ? inspection : null
  const inspectionContext = open || current !== null ? context : null
  const reviewed = useMemo(
    () =>
      settings === null
        ? { rows: [], blockers: [], outputs: null, offsets: {} }
        : reviewExport({
            outputMode,
            exportMode: settings.exportMode,
            clips,
            exportRange: settings.range,
            transitionDefaults: inputs.transitionDefaults,
            skipOverlay: store.batchSkipOverlay,
            inspection: current,
            sync,
            locale: i18n.resolvedLanguage,
            outputPath,
          }),
    [clips, current, i18n.resolvedLanguage, inputs.transitionDefaults, outputMode, outputPath, settings, store.batchSkipOverlay, sync],
  )
  // Review the destination without replacing an existing combined file; submission asks first.
  const destination = useMemo(
    () => (outputMode === null ? null : createExportDestination({ outputMode, outputPath, outputDirectory, overwrite: true })),
    [outputMode, outputPath, outputDirectory],
  )
  const review = useMemo(
    () => ({ current, reviewed, sync, destination, encoding: settings ? captureRenderEncoding(settings, inputs.availableCodecs) : null }),
    [current, reviewed, sync, destination, settings, inputs.availableCodecs],
  )

  useEffect(() => {
    if (inspectionContext === null || inspectionContext.sourcePaths === '') return
    const context = inspectionContext
    const paths = context.sourcePaths.split('\n')
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
        const sources = new Map()
        // Requests are independent; Rust bounds active probes across all sessions.
        await Promise.all(
          paths.map(async (path) => {
            try {
              sources.set(path, { source: await backend.inspectVideoSource(inspectionId, path), error: null })
            } catch (error) {
              sources.set(path, { source: null, error })
            }
            if (!closed) setInspection({ context, inspectionId, sources: new Map(sources), complete: false, error: null })
          }),
        )
        if (!closed) setInspection({ context, inspectionId, sources, complete: true, error: null })
      } catch (error) {
        if (!closed) setInspection({ context, error, sources: new Map() })
      }
    })()
    return () => {
      closed = true
      if (inspectionId !== null)
        void backend.disposeVideoInspection(inspectionId).catch((error) => useStore.getState().setErrorMessage(error.message))
    }
  }, [inspectionContext])

  useEffect(() => {
    if (!open || !current?.complete || current.error || sync.error || reviewed.outputs === null || reviewed.outputs.length === 0) return
    if (destination === null) return
    let closed = false
    void backend
      .planBatchOutputs({ inspectionId: current.inspectionId, outputs: reviewed.outputs, destination }, review.encoding)
      .then((result) => {
        if (closed) return
        if (result.status === 'rejected') setInspection({ ...current, error: new Error('reinspectionRequired'), issues: result.issues })
        else setPlan({ review, ...result })
      })
      .catch((error) => {
        if (!closed) setPlan({ review, status: 'error', error })
      })
    return () => {
      closed = true
    }
  }, [open, current, sync, reviewed, destination, review])

  const currentPlan = plan?.review === review ? plan : null
  const rowsWithIssues = useMemo(() => {
    const staleIds = new Set((current?.issues ?? []).map((issue) => issue.sourceId))
    return reviewed.rows.map((row) => (staleIds.has(row.source?.sourceId) ? { ...row, status: 'blocked', error: 'reinspectionRequired' } : row))
  }, [reviewed.rows, current?.issues])
  const ready = open && currentPlan?.status === 'planned'
  const batchRunning = store.batchRunning || store.batchSubmissionPending
  const pickOutputFolder = async () => {
    const directory = await openDirectoryPath({ lastDirectoryKey: 'last-batch-output-dir' })
    if (directory !== null) store.setBatchOutputFolder(directory)
  }
  return {
    ...store,
    batchRunning,
    batchInspecting: clips.length > 0 && (current === null || (!current.complete && !current.error)),
    batchQueue: rowsWithIssues,
    batchReady: ready && !batchRunning,
    batchReviewError: formatRenderPathError(current?.error ?? sync.error ?? null, t),
    batchOutputError: formatRenderPathError(currentPlan?.error ?? null, t),
    combinedBlockers: reviewed.blockers,
    request: ready ? { inspectionId: current.inspectionId, outputs: reviewed.outputs, offsets: reviewed.offsets, reviewedInputs: inputs } : null,
    pickOutputFolder,
    refreshInspection: () => setRevision((value) => value + 1),
    reject: (error) => setInspection({ ...current, error: new Error('reinspectionRequired'), issues: error.issues }),
  }
}
