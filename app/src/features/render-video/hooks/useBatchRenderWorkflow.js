/**
 * Batch render workflow — imports videos from a folder one at a time into the
 * existing single-video import/sync pipeline, then renders each sequentially
 * into a shared output folder using the render dialog's settings draft.
 * Composed by useRenderVideoDialogState when the dialog targets a batch.
 */

import { useCallback, useRef, useState } from 'react'
import * as backend from '@/api/backend'
import { DEFAULT_EXPORT_RANGE } from '@/lib/template/template-constants'
import { openDirectoryPath } from '@/lib/file-dialog'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { pathInDirectory } from '@/lib/utils'
import useStore from '@/store/useStore'
import { resolveVideoSyncState } from '@/store/slices/createVideoImportSlice'
import { runWithoutEditorHistory } from '@/features/undo-redo/undoHistory'
import useVideoImport, { prepareVideoPath } from '@/features/video-preview/hooks/useVideoImport'
import { getRenderOutputExtension } from '../utils/render-output'

function outputFilenameFor(filename, exportMode) {
  const stem = filename.replace(/\.[^.]*$/, '')
  return `${stem}.${getRenderOutputExtension(exportMode)}`
}

const OVERLAP_CHECK_CONCURRENCY = 4

// Mirrors the Video sync "Apply Timezone" switch, which shows unchecked for anything but 'utc'.
function selectedTimezoneMode(state) {
  return state.videoSyncTimezoneMode === 'utc' ? 'utc' : 'local'
}

// Runs `worker` over `items` with at most `limit` in flight at once, so
// probing a large queue doesn't spawn one ffprobe process per video at once.
async function runWithConcurrencyLimit(items, limit, worker) {
  let cursor = 0
  const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (cursor < items.length) {
      const index = cursor++
      await worker(items[index], index)
    }
  })
  await Promise.all(runners)
}

function waitForRenderCompletion(renderId) {
  return new Promise((resolve, reject) => {
    let unlisten = null
    let settled = false

    const finish = (fn) => {
      if (settled) return
      settled = true
      if (unlisten) unlisten()
      fn()
    }

    const handle = (data) => {
      if (data.render_id !== renderId) return
      if (data.status === 'complete') finish(() => resolve(data))
      else if (data.status === 'cancelled') finish(() => reject(Object.assign(new Error('Render cancelled'), { code: 'cancelled' })))
      else if (data.status === 'error') finish(() => reject(new Error(data.message || 'Render failed')))
    }

    backend
      .subscribeRenderProgress(handle)
      .then((un) => {
        unlisten = un
      })
      .catch((error) => finish(() => reject(error)))

    backend
      .getRenderProgress()
      .then(handle)
      .catch(() => {})
  })
}

/**
 * Provides batch-render queue actions and sequential render orchestration.
 *
 * @param {object} params
 * @param {object|null} params.settings - Render dialog settings draft applied to every queued video.
 * @returns {object} Batch render workflow API.
 */
export default function useBatchRenderWorkflow({ settings }) {
  const batchVideoFolder = useStore((state) => state.batchVideoFolder)
  const batchOutputFolder = useStore((state) => state.batchOutputFolder)
  const batchQueue = useStore((state) => state.batchQueue)
  const batchRunning = useStore((state) => state.batchRunning)
  const batchActiveItemId = useStore((state) => state.batchActiveItemId)
  const setBatchVideoFolder = useStore((state) => state.setBatchVideoFolder)
  const setBatchOutputFolder = useStore((state) => state.setBatchOutputFolder)
  const setBatchQueueFromPaths = useStore((state) => state.setBatchQueueFromPaths)
  const removeBatchQueueItem = useStore((state) => state.removeBatchQueueItem)
  const clearBatchQueue = useStore((state) => state.clearBatchQueue)
  const setBatchItemSkipOverlay = useStore((state) => state.setBatchItemSkipOverlay)
  const setBatchItemStatus = useStore((state) => state.setBatchItemStatus)
  const setBatchRunning = useStore((state) => state.setBatchRunning)
  const setBatchActiveItemId = useStore((state) => state.setBatchActiveItemId)
  const setErrorMessage = useStore((state) => state.setErrorMessage)
  const setRenderSettings = useStore((state) => state.setRenderSettings)

  const { loadVideoPath, clearImportedVideo } = useVideoImport({})
  const [currentItemProgress, setCurrentItemProgress] = useState(null)
  const cancelRequestedRef = useRef(false)

  // Probes each queued video's creation time against the loaded activity so
  // the per-item overlay toggle defaults correctly without a manual render.
  const detectQueueOverlaps = useCallback(
    async (paths) => {
      const activitySummary = useStore.getState().activitySummary
      if (!activitySummary) return

      const timezoneMode = selectedTimezoneMode(useStore.getState())
      const itemsByPath = new Map(useStore.getState().batchQueue.map((candidate) => [candidate.path, candidate]))
      const items = paths.map((path) => itemsByPath.get(path)).filter(Boolean)
      for (const item of items) setBatchItemStatus(item.id, 'checking')

      await runWithConcurrencyLimit(items, OVERLAP_CHECK_CONCURRENCY, async (item) => {
        try {
          const { importedVideoState } = await prepareVideoPath(item.path)
          const { videoSyncWarning } = resolveVideoSyncState({ ...importedVideoState, videoSyncTimezoneMode: timezoneMode }, activitySummary)
          setBatchItemSkipOverlay(item.id, videoSyncWarning !== null)
          setBatchItemStatus(item.id, 'pending')
        } catch (error) {
          console.warn(`Could not determine activity overlap for ${item.path}:`, error)
          setBatchItemStatus(item.id, 'error', 'Could not check activity overlap')
        }
      })
    },
    [setBatchItemSkipOverlay, setBatchItemStatus],
  )

  const pickVideoFolder = useCallback(async () => {
    const directory = await openDirectoryPath({ lastDirectoryKey: 'last-batch-video-dir' })
    if (!directory) return
    const paths = await backend.listDirectoryVideoFiles(directory)
    setBatchVideoFolder(directory)
    setBatchQueueFromPaths(paths)
    void detectQueueOverlaps(paths)
  }, [detectQueueOverlaps, setBatchQueueFromPaths, setBatchVideoFolder])

  const pickOutputFolder = useCallback(async () => {
    const directory = await openDirectoryPath({ lastDirectoryKey: 'last-batch-output-dir' })
    if (!directory) return
    setBatchOutputFolder(directory)
  }, [setBatchOutputFolder])

  const renderQueueItem = useCallback(
    async (item, batchSettings, timezoneMode) => {
      setBatchActiveItemId(item.id)
      setBatchItemStatus(item.id, 'importing')
      setCurrentItemProgress(null)
      await loadVideoPath(item.path)

      const { parsedActivitySource, setVideoSyncTimezoneMode } = useStore.getState()
      if (parsedActivitySource === 'activity-file') {
        await runWithoutEditorHistory(useStore, () => setVideoSyncTimezoneMode(timezoneMode))
      }

      const state = useStore.getState()
      if (!state.parsedActivity) {
        throw new Error('No activity is loaded')
      }
      if (!state.config?.scene) {
        throw new Error('No template is loaded')
      }

      const { exportMode } = batchSettings
      const shouldComposite = exportMode === 'composite'
      const itemConfig = item.skipOverlay ? { ...state.config, values: [], plots: [] } : state.config
      const effectiveConfig = { ...itemConfig, scene: { ...itemConfig.scene, fps: batchSettings.fps } }
      const outputPath = pathInDirectory(batchOutputFolder, outputFilenameFor(item.filename, exportMode))
      const updateRate = normalizeUpdateRateForFps(shouldComposite ? state.importedVideoFps : batchSettings.fps, batchSettings.updateRate)

      setBatchItemStatus(item.id, 'rendering')
      const { default: submitRenderVideo } = await import('@/features/render-video/utils/render-video')
      const result = await submitRenderVideo({
        config: effectiveConfig,
        exportMode,
        exportCodec: batchSettings.exportCodec,
        exportBitrate: batchSettings.exportBitrate,
        exportRange: DEFAULT_EXPORT_RANGE,
        updateRate,
        availableCodecs: state.availableCodecs,
        globalDefaults: state.globalDefaults,
        importedVideoDuration: state.importedVideoDuration,
        importedVideoFps: state.importedVideoFps,
        importedVideoFpsNum: state.importedVideoFpsNum,
        importedVideoFpsDen: state.importedVideoFpsDen,
        importedVideoPath: shouldComposite ? state.importedVideoPath : null,
        importedVideoResolution: state.importedVideoResolution,
        parsedActivity: state.parsedActivity,
        startSecond: state.startSecond,
        endSecond: state.endSecond,
        videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
        outputPath,
        overwrite: true,
      })
      let unlisten = null
      backend
        .subscribeRenderProgress((data) => {
          if (data.render_id === result.render_id) setCurrentItemProgress(data)
        })
        .then((un) => {
          unlisten = un
        })
        .catch(() => {})

      try {
        await waitForRenderCompletion(result.render_id)
        setBatchItemStatus(item.id, 'done')
      } finally {
        if (unlisten) unlisten()
        setCurrentItemProgress(null)
      }
    },
    [batchOutputFolder, loadVideoPath, setBatchActiveItemId, setBatchItemStatus],
  )

  const runBatch = useCallback(async () => {
    if (!batchOutputFolder) {
      setErrorMessage('Select a batch output folder first')
      return
    }
    if (batchQueue.length === 0) return

    cancelRequestedRef.current = false
    // Captured once: importing each queued video resets the editor's selection
    // and may re-normalize the live dialog draft.
    const timezoneMode = selectedTimezoneMode(useStore.getState())
    const batchSettings = settings
    setRenderSettings({
      ...useStore.getState().renderSettings,
      fps: batchSettings.fps,
      widgetUpdateRate: batchSettings.updateRate,
      exportMode: batchSettings.exportMode,
      codec: batchSettings.exportCodec,
      bitrateMbps: batchSettings.exportBitrate ?? null,
    })
    setBatchRunning(true)
    try {
      for (const item of batchQueue) {
        if (cancelRequestedRef.current) {
          setBatchItemStatus(item.id, 'pending')
          continue
        }
        try {
          await renderQueueItem(item, batchSettings, timezoneMode)
        } catch (error) {
          setBatchItemStatus(item.id, error?.code === 'cancelled' ? 'cancelled' : 'error', error?.message || 'Render failed')
          if (error?.code === 'cancelled') cancelRequestedRef.current = true
        }
      }
    } finally {
      setBatchActiveItemId(null)
      setBatchRunning(false)
      try {
        await clearImportedVideo()
      } catch {
        // best-effort cleanup
      }
    }
  }, [
    batchOutputFolder,
    batchQueue,
    clearImportedVideo,
    renderQueueItem,
    setBatchActiveItemId,
    setBatchItemStatus,
    setBatchRunning,
    setErrorMessage,
    setRenderSettings,
    settings,
  ])

  const cancelBatch = useCallback(async () => {
    cancelRequestedRef.current = true
    try {
      await backend.cancelRender()
    } catch (error) {
      console.error('Failed to cancel batch render:', error)
    }
  }, [])

  return {
    batchVideoFolder,
    batchOutputFolder,
    batchQueue,
    batchRunning,
    batchActiveItemId,
    currentItemProgress,
    pickVideoFolder,
    pickOutputFolder,
    removeBatchQueueItem,
    clearBatchQueue,
    setBatchItemSkipOverlay,
    runBatch,
    cancelBatch,
  }
}
