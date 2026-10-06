import { useEffect, useMemo } from 'react'
import * as backend from '@/api/backend'
import { openDirectoryPath } from '@/lib/file-dialog'
import { useBatchRenderStore, useBatchSyncInputs } from '@/hooks/useAppStoreSelectors'
import useStore from '@/store/useStore'
import { runWithoutEditorHistory } from '@/features/undo-redo/undoHistory'
import { createBatchRenderRequest } from '../utils/batchRenderRequest'
import { submitObservedBatch } from '../utils/batchRenderExecution'
import { batchResultQueue, isBatchFinished, reviewBatchSync } from '../utils/batchRenderReview'
import { createBatchProgress, createBatchItemProgress } from '../utils/renderProgress'
import useBatchInspection from './useBatchInspection'

/** @param {object} params Dialog phase and settings. @returns {object} Batch controls and native status presentation. */
export default function useBatchRenderWorkflow({ phase, settings }) {
  const store = useBatchRenderStore()
  const inputs = useBatchSyncInputs()
  const open = phase === 'confirm' && settings?.renderTarget === 'batch'
  const sync = useMemo(() => reviewBatchSync(inputs), [inputs])
  const review = useBatchInspection({
    open: open && store.batchSnapshot === null,
    folder: store.batchVideoFolder,
    outputDirectory: store.batchOutputFolder,
    choices: store.batchQueue,
    sync,
    settings,
    availableCodecs: inputs.availableCodecs,
  })
  const batchRunning = store.batchRunning || store.batchSubmissionPending

  useEffect(() => {
    const state = useStore.getState()
    if (!open && !state.batchSnapshot?.rendererBusy && !state.batchSubmissionPending) state.clearBatchResults()
  }, [open])

  // Recover after a remount. The submission listener remains attached through native cleanup.
  const batchId = store.batchSnapshot?.batchId
  useEffect(() => {
    if (batchId === undefined) return
    void backend
      .getBatchRenderSnapshot(batchId)
      .then((snapshot) => useStore.getState().applyBatchSnapshot(snapshot))
      .catch((error) => useStore.getState().setErrorMessage(error.message))
  }, [batchId])

  const runBatch = async () => {
    const state = useStore.getState()
    if (!review.ready || state.batchSnapshot?.rendererBusy || state.batchSubmissionPending) return
    state.setBatchSubmissionPending(true)
    try {
      const request = createBatchRenderRequest({
        editorSnapshot: state,
        settings,
        inspectionId: review.inspection.inspectionId,
        outputDirectory: store.batchOutputFolder,
        jobs: review.jobs,
        calibrationSource: review.inspection.calibrationSource,
      })
      await submitObservedBatch(request)
      await runWithoutEditorHistory(useStore, () =>
        useStore.getState().setRenderSettings({
          ...state.renderSettings,
          fps: request.encoding.fps,
          widgetUpdateRate: request.encoding.updateRate,
          exportMode: request.encoding.exportMode,
          codec: request.encoding.exportCodec,
          qualityType: request.encoding.qualityType,
          qualityValue: request.encoding.qualityValue,
        }),
      )
    } catch (error) {
      if (error.code === 'reinspectionRequired') review.reject(error)
      else state.setErrorMessage(error.message)
    } finally {
      useStore.getState().setBatchSubmissionPending(false)
    }
  }
  const cancelBatch = async () => {
    try {
      useStore.getState().applyBatchSnapshot(await backend.cancelBatchRender(store.batchSnapshot.batchId))
    } catch (error) {
      store.setErrorMessage(error.message)
    }
  }
  const pickVideoFolder = async () => {
    const directory = await openDirectoryPath({ lastDirectoryKey: 'last-batch-video-dir' })
    if (directory !== null) {
      store.setBatchVideoFolder(directory)
    }
  }
  const pickOutputFolder = async () => {
    const directory = await openDirectoryPath({ lastDirectoryKey: 'last-batch-output-dir' })
    if (directory !== null) store.setBatchOutputFolder(directory)
  }

  return {
    ...store,
    batchRunning,
    batchQueue: store.batchSnapshot === null ? review.rows : batchResultQueue(store.batchSnapshot),
    batchReady: review.ready && !batchRunning,
    batchFinished: isBatchFinished(store.batchSnapshot),
    batchActiveItemId: store.batchSnapshot?.activeItemId ?? null,
    batchReviewError: review.error,
    currentItemProgress: createBatchItemProgress(store.batchSnapshot),
    batchProgress: createBatchProgress(store.batchSnapshot),
    runBatch,
    cancelBatch,
    pickVideoFolder,
    pickOutputFolder,
    refreshInspection: review.refresh,
  }
}
