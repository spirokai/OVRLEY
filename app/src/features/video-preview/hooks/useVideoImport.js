/**
 * Video import - background media selection and preview management.
 */

import { useMemo, useState } from 'react'
import { clearPreviewVideos, extractVideoTelemetry, importPreviewVideo, preparePreviewVideo, removePreviewVideo } from '@/api/backend'
import { changeVideoCollection, runWithoutEditorHistory } from '@/features/undo-redo/undoHistory'
import { openMultiplePaths } from '@/lib/file-dialog'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { deriveCompatibility, joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import { createImportedVideo, nextImportOrder } from '@/store/slices/createVideoImportSlice'
import useStore from '@/store/useStore'
import i18next from 'i18next'

const VIDEO_EXTENSIONS = new Set(['mp4', 'mov', 'mkv'])
const DEBUG_IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'webp'])

function pathExtension(path) {
  return typeof path === 'string' ? path.split('.').pop()?.toLowerCase() || '' : ''
}

function isVideoPath(path) {
  return VIDEO_EXTENSIONS.has(pathExtension(path))
}

/**
 * Extracts embedded telemetry as a settled per-video result. Missing or failed
 * telemetry is a valid video state and never prevents playback.
 * @param {string} path Absolute video path.
 * @returns {Promise<{status: 'ready'|'absent'|'failed', activity: object|null}>} Settled telemetry.
 */
async function extractTelemetryResult(path) {
  try {
    const response = await extractVideoTelemetry(path)
    return response?.parsed_activity ? { status: 'ready', activity: response.parsed_activity } : { status: 'absent', activity: null }
  } catch (error) {
    console.warn('MP4 telemetry extraction failed (non-fatal):', error)
    return { status: 'failed', activity: null }
  }
}

let telemetryQueue = Promise.resolve()

/**
 * Queues embedded telemetry extraction for imported clips. Extraction runs one clip
 * at a time in timeline order; results for clips removed meanwhile are discarded.
 * @param {Array<object>} videos Newly imported video entities.
 * @returns {Promise<void>} Completion of this batch of the queue.
 */
function queueTelemetryExtraction(videos) {
  const { videoEdits } = useStore.getState()
  const ordered = sortVideosByTimeline(joinVideoEdits(videos, videoEdits))
  telemetryQueue = telemetryQueue
    .then(async () => {
      for (const video of ordered) {
        if (!useStore.getState().importedVideos.some((candidate) => candidate.id === video.id)) continue
        const telemetry = await extractTelemetryResult(video.path)
        runWithoutEditorHistory(useStore, () => useStore.getState().setVideoTelemetry(video.id, telemetry))
      }
    })
    // Later imports must still get their telemetry after an unexpected failure.
    .catch((error) => useStore.getState().setErrorMessage(`Video telemetry failed: ${errorMessage(error)}`))
  return telemetryQueue
}

/**
 * Probes a project video without mutating preview-server or application state.
 * @param {string} path Absolute video path.
 * @returns {Promise<{path: string, metadata: object, previewWarnings: Array<string>, telemetry: {status: string, activity: object|null}}>} Prepared video source.
 */
export async function prepareVideoPath(path) {
  if (typeof path !== 'string' || path.length === 0) throw new Error('Video import requires a file path.')
  if (!isVideoPath(path)) throw new Error('Selected file is not a supported video.')

  const [preview, telemetry] = await Promise.all([preparePreviewVideo(path), extractTelemetryResult(path)])
  return { path, metadata: preview.metadata, previewWarnings: preview.warnings, telemetry }
}

/**
 * Sizes the scene and render FPS to the compatible format of the first import.
 * @param {{resolution: {width: number, height: number}, fps: {num: number, den: number}}} format Reference format.
 */
function adoptVideoFormat(format) {
  const { config, renderSettings, setConfig, setRenderFpsAndUpdateRate } = useStore.getState()
  if (!config?.scene) {
    throw new Error('Cannot import video without an active template scene')
  }
  setConfig({ ...config, scene: { ...config.scene, width: format.resolution.width, height: format.resolution.height } })
  const fps = Math.round(format.fps.num / format.fps.den)
  setRenderFpsAndUpdateRate(fps, normalizeUpdateRateForFps(fps, renderSettings.widgetUpdateRate))
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error)
}

/**
 * Probes and registers every selected file, then adds the successful ones to the
 * collection in one change. A file that is not a video, is already imported, or
 * fails to probe is reported and skipped; the others still import.
 * @param {Array<string>} paths Absolute file paths in selection order.
 * @returns {Promise<{videos: Array<object>, failures: Array<{path: string, message: string}>, duplicates: Array<string>}>} Import result.
 */
async function importVideoPaths(paths) {
  const state = useStore.getState()
  const knownPaths = new Set(state.importedVideos.map((video) => video.path))
  const wasEmpty = state.importedVideos.length === 0
  let importOrder = nextImportOrder(state.importedVideos)
  const videos = []
  const failures = []
  const duplicates = []

  for (const path of paths) {
    if (!isVideoPath(path)) {
      failures.push({ path, message: i18next.t('video-preview.unsupportedVideoFile', 'Not a supported video file (MP4, MOV, MKV).') })
      continue
    }
    if (knownPaths.has(path)) {
      duplicates.push(path)
      continue
    }
    knownPaths.add(path)
    let registration
    try {
      registration = await importPreviewVideo(path)
      videos.push(
        createImportedVideo(registration.metadata, {
          importOrder,
          preview: { importId: registration.importId, url: registration.previewUrl, warnings: registration.warnings },
        }),
      )
      importOrder += 1
    } catch (error) {
      if (registration) await removePreviewVideo(registration.importId)
      failures.push({ path, message: errorMessage(error) })
    }
  }

  if (videos.length > 0) {
    try {
      changeVideoCollection(useStore, () => {
        useStore.getState().addImportedVideos(videos)
        if (wasEmpty) adoptVideoFormat(deriveCompatibility(videos).reference)
      })
    } catch (error) {
      for (const video of videos) await removePreviewVideo(video.preview.importId)
      throw error
    }
  }
  return { videos, failures, duplicates }
}

/**
 * Describes what an import needs to tell the user: files that failed or were
 * already imported, and clips that cannot be combined into one output when the
 * import made the collection incompatible in a new way.
 * @returns {object|null} Import report, or null when there is nothing to report.
 */
function createImportReport({ failures, duplicates }, incompatibleBefore) {
  const { importedVideos } = useStore.getState()
  const { reference, incompatibleIds } = deriveCompatibility(importedVideos)
  const newlyIncompatible = [...incompatibleIds].some((id) => !incompatibleBefore.has(id))
  const incompatibleVideos = newlyIncompatible ? importedVideos.filter((video) => incompatibleIds.has(video.id)) : []
  if (failures.length === 0 && duplicates.length === 0 && incompatibleVideos.length === 0) return null
  return { failures, duplicates, reference: incompatibleVideos.length > 0 ? reference : null, incompatibleVideos }
}

function basename(path) {
  return path.split(/[/\\]/).pop()
}

/**
 * Summarizes one imported video for drawers and project views.
 * @param {object} video Imported video entity.
 * @returns {object} Presentational video summary.
 */
function createVideoSummary(video) {
  return {
    id: video.id,
    path: video.path,
    filename: basename(video.path),
    duration: video.durationSeconds,
    fps: video.fps.num / video.fps.den,
    resolution: video.resolution,
    creationTime: video.creationTime,
    timeSource: video.timeSource,
    codecName: video.codecName,
    codecLongName: video.codecLongName,
    bitRate: video.bitRate,
    cameraType: video.cameraType,
    cameraModel: video.cameraModel,
  }
}

export default function useVideoImport({ debugModeEnabled = false, onSetBackgroundMode }) {
  const importedVideos = useStore((state) => state.importedVideos)
  const videoEdits = useStore((state) => state.videoEdits)
  const importedBackgroundImagePath = useStore((state) => state.importedBackgroundImagePath)
  const setImportedBackgroundImage = useStore((state) => state.setImportedBackgroundImage)
  const clearImportedVideos = useStore((state) => state.clearImportedVideos)
  const setImportingVideo = useStore((state) => state.setImportingVideo)
  const setErrorMessage = useStore((state) => state.setErrorMessage)

  const videoSummaries = useMemo(
    () => sortVideosByTimeline(joinVideoEdits(importedVideos, videoEdits)).map(createVideoSummary),
    [importedVideos, videoEdits],
  )
  const hasVideos = videoSummaries.length > 0
  const importedVideoLabel =
    videoSummaries.length > 1
      ? i18next.t('video-preview.videoCount', { defaultValue: '{{count}} videos', count: videoSummaries.length })
      : (videoSummaries[0]?.filename ?? null)
  const importedBackgroundImageFilename = importedBackgroundImagePath ? basename(importedBackgroundImagePath) : null
  const importedMediaFilename = importedBackgroundImageFilename || importedVideoLabel

  const [importReport, setImportReport] = useState(null)

  /**
   * Imports the given paths behind the blocking loader, then reports failures,
   * duplicates, and new incompatibilities. Telemetry extraction continues afterwards.
   */
  const loadVideoPaths = async (paths) => {
    const incompatibleBefore = deriveCompatibility(useStore.getState().importedVideos).incompatibleIds
    setImportingVideo(true)
    let result
    try {
      result = await importVideoPaths(paths)
    } finally {
      setImportingVideo(false)
    }
    if (result.videos.length > 0) {
      onSetBackgroundMode?.('video')
      void queueTelemetryExtraction(result.videos)
    }
    setImportReport(createImportReport(result, incompatibleBefore))
  }

  const applyBackgroundImage = async (path) => {
    setImportingVideo(true)
    try {
      if (hasVideos) {
        await clearPreviewVideos()
      }
      changeVideoCollection(useStore, () => setImportedBackgroundImage(path))
      onSetBackgroundMode?.('image')
    } finally {
      setImportingVideo(false)
    }
  }

  const handleImportVideo = async () => {
    try {
      const selected = await openMultiplePaths(
        [
          {
            name: debugModeEnabled ? i18next.t('video-preview.videoOrImage', 'Video or Image') : 'Video',
            extensions: debugModeEnabled ? ['mp4', 'mov', 'mkv', 'png', 'jpg', 'jpeg', 'webp'] : ['mp4', 'mov', 'mkv'],
          },
        ],
        { lastDirectoryKey: 'last-video-import-dir' },
      )
      if (!selected) {
        return
      }

      if (debugModeEnabled && selected.length === 1 && DEBUG_IMAGE_EXTENSIONS.has(pathExtension(selected[0]))) {
        await applyBackgroundImage(selected[0])
        return
      }

      await loadVideoPaths(selected)
    } catch (err) {
      console.error('Failed to import background media:', err)
      setErrorMessage(`Video import failed: ${err.message}`)
    }
  }

  const handleVideoFilesDrop = async (selections) => {
    try {
      // Only native drops carry file paths; browser File objects cannot be registered.
      if (selections.some((selection) => typeof selection !== 'string')) {
        throw new Error('Dropped files must be local files.')
      }
      if (debugModeEnabled && selections.length === 1 && DEBUG_IMAGE_EXTENSIONS.has(pathExtension(selections[0]))) {
        await applyBackgroundImage(selections[0])
        return
      }

      await loadVideoPaths(selections)
    } catch (err) {
      console.error('Video drop failed:', err)
      setErrorMessage(`Video drop failed: ${err.message}`)
    }
  }

  const handleClearImportedVideos = async () => {
    try {
      if (hasVideos) {
        await clearPreviewVideos()
      }
    } catch (err) {
      console.error('Failed to clear preview videos:', err)
    } finally {
      changeVideoCollection(useStore, () => clearImportedVideos())
      onSetBackgroundMode?.('checker')
    }
  }

  return {
    debugModeEnabled,
    hasVideos,
    importedBackgroundImageFilename,
    importedMediaFilename,
    importedVideoLabel,
    videoSummaries,
    clearImportedVideos: handleClearImportedVideos,
    handleImportVideo,
    handleVideoFilesDrop,
    importReportDialog: {
      open: importReport !== null,
      report: importReport,
      onClose: () => setImportReport(null),
    },
    loadVideoPaths,
    prepareVideoPath,
  }
}
