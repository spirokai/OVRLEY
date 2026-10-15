/**
 * Video import - background media selection and preview management.
 */

import { useMemo } from 'react'
import { clearPreviewVideos, extractVideoTelemetry, importPreviewVideo, preparePreviewVideo, removePreviewVideo } from '@/api/backend'
import { changeVideoCollection, runWithoutEditorHistory } from '@/features/undo-redo/undoHistory'
import { openSinglePath } from '@/lib/file-dialog'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import { createImportedVideo } from '@/store/slices/createVideoImportSlice'
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

async function extractAndStoreVideoTelemetry(videoId, path) {
  const telemetry = await extractTelemetryResult(path)
  await runWithoutEditorHistory(useStore, () => useStore.getState().setVideoTelemetry(videoId, telemetry))
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
 * Sizes the scene and render FPS to an imported video.
 * @param {object} video Imported video entity.
 */
function adoptVideoFormat(video) {
  const { config, renderSettings, setConfig, setRenderFpsAndUpdateRate } = useStore.getState()
  if (!config?.scene) {
    throw new Error('Cannot import video without an active template scene')
  }
  setConfig({ ...config, scene: { ...config.scene, width: video.resolution.width, height: video.resolution.height } })
  const fps = Math.round(video.fps.num / video.fps.den)
  setRenderFpsAndUpdateRate(fps, normalizeUpdateRateForFps(fps, renderSettings.widgetUpdateRate))
}

async function importVideoSelection(selection, { setImportingVideo, onSetBackgroundMode }) {
  if (typeof selection !== 'string') {
    throw new Error('Video import requires a file path.')
  }

  const path = selection
  if (path.length === 0) {
    throw new Error('Video import requires a file path.')
  }

  if (!isVideoPath(path)) {
    throw new Error('Dropped file is not a supported video.')
  }

  setImportingVideo(true)

  const previousImportIds = useStore.getState().importedVideos.map((video) => video.preview.importId)
  const response = await importPreviewVideo(path)
  let video
  try {
    video = createImportedVideo(response.metadata, {
      importOrder: 0,
      preview: { importId: response.importId, url: response.previewUrl, warnings: response.warnings },
    })
    changeVideoCollection(useStore, () => {
      useStore.getState().setImportedVideos([video])
      adoptVideoFormat(video)
    })
  } catch (error) {
    await removePreviewVideo(response.importId)
    throw error
  }
  for (const importId of previousImportIds) await removePreviewVideo(importId)
  onSetBackgroundMode?.('video')

  await extractAndStoreVideoTelemetry(video.id, path)
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

  const loadVideoPath = async (path) => {
    try {
      await importVideoSelection(path, { setImportingVideo, onSetBackgroundMode })
    } finally {
      setImportingVideo(false)
    }
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
      const selected = await openSinglePath(
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

      if (debugModeEnabled && DEBUG_IMAGE_EXTENSIONS.has(pathExtension(selected))) {
        await applyBackgroundImage(selected)
        return
      }

      await loadVideoPath(selected)
    } catch (err) {
      console.error('Failed to import background media:', err)
      setErrorMessage(`Video import failed: ${err.message}`)
    } finally {
      setImportingVideo(false)
    }
  }

  const handleVideoFilesDrop = async (selections) => {
    try {
      if (selections.length !== 1) {
        throw new Error('Drop exactly one video file.')
      }

      const selection = selections[0]
      const path = typeof selection === 'string' ? selection : selection.name
      if (debugModeEnabled && typeof path === 'string' && DEBUG_IMAGE_EXTENSIONS.has(pathExtension(path))) {
        await applyBackgroundImage(path)
        return
      }

      await loadVideoPath(selection)
    } catch (err) {
      console.error('Video drop failed:', err)
      setErrorMessage(`Video drop failed: ${err.message}`)
    } finally {
      setImportingVideo(false)
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
    loadVideoPath,
    prepareVideoPath,
  }
}
