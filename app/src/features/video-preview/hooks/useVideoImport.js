/**
 * Video import - background media selection and preview management.
 */

import { clearPreviewVideo, extractVideoTelemetry, importPreviewVideo } from '@/api/backend'
import { runWithoutEditorHistory } from '@/features/undo-redo/undoHistory'
import { openSinglePath } from '@/lib/file-dialog'
import useStore from '@/store/useStore'

const VIDEO_EXTENSIONS = new Set(['mp4', 'mov', 'mkv'])
const DEBUG_IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'webp'])

function pathExtension(path) {
  return typeof path === 'string' ? path.split('.').pop()?.toLowerCase() || '' : ''
}

function isVideoPath(path) {
  return VIDEO_EXTENSIONS.has(pathExtension(path))
}

async function extractAndStoreVideoTelemetry(filePath) {
  try {
    const response = await extractVideoTelemetry(filePath)
    if (response?.parsed_activity) {
      await runWithoutEditorHistory(useStore, () => useStore.getState().loadVideoTelemetry(response.parsed_activity))
    }
  } catch (error) {
    console.warn('MP4 telemetry extraction failed (non-fatal):', error)
  }
}

async function importVideoSelection(selection, { setImportingVideo, setImportedVideo, setConfig, clearVideoTelemetry, onSetBackgroundMode }) {
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
  clearVideoTelemetry()

  const response = await importPreviewVideo(path)
  const metadata = {
    ...response.metadata,
    importId: response.importId,
    previewUrl: response.previewUrl,
    previewWarnings: response.warnings ?? [],
  }
  const currentConfig = useStore.getState().config
  if (!currentConfig?.scene) {
    throw new Error('Cannot import video without an active template scene')
  }

  await runWithoutEditorHistory(useStore, () => {
    const importedVideoResolution = setImportedVideo(metadata)
    setConfig({
      ...currentConfig,
      scene: {
        ...currentConfig.scene,
        ...(metadata.fps ? { fps: Math.round(metadata.fps) } : {}),
        width: importedVideoResolution.width,
        height: importedVideoResolution.height,
      },
    })
  })
  onSetBackgroundMode?.('video')

  void extractAndStoreVideoTelemetry(path)
}

export default function useVideoImport({ debugModeEnabled = false, onSetBackgroundMode }) {
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoDuration = useStore((state) => state.importedVideoDuration)
  const importedVideoFps = useStore((state) => state.importedVideoFps)
  const importedVideoResolution = useStore((state) => state.importedVideoResolution)
  const importedVideoCreationTime = useStore((state) => state.importedVideoCreationTime)
  const importedVideoTimeSource = useStore((state) => state.importedVideoTimeSource)
  const importedVideoCodecName = useStore((state) => state.importedVideoCodecName)
  const importedVideoCodecLongName = useStore((state) => state.importedVideoCodecLongName)
  const importedVideoBitRate = useStore((state) => state.importedVideoBitRate)
  const importedVideoCameraType = useStore((state) => state.importedVideoCameraType)
  const importedVideoCameraModel = useStore((state) => state.importedVideoCameraModel)
  const importedBackgroundImagePath = useStore((state) => state.importedBackgroundImagePath)
  const setImportedVideo = useStore((state) => state.setImportedVideo)
  const setImportedBackgroundImage = useStore((state) => state.setImportedBackgroundImage)
  const clearImportedVideo = useStore((state) => state.clearImportedVideo)
  const setImportingVideo = useStore((state) => state.setImportingVideo)
  const setConfig = useStore((state) => state.setConfig)
  const clearVideoTelemetry = useStore((state) => state.clearVideoTelemetry)
  const setErrorMessage = useStore((state) => state.setErrorMessage)

  const importedVideoFilename = importedVideoPath ? importedVideoPath.split(/[/\\]/).pop() : null
  const importedBackgroundImageFilename = importedBackgroundImagePath ? importedBackgroundImagePath.split(/[/\\]/).pop() : null
  const importedMediaFilename = importedBackgroundImageFilename || importedVideoFilename

  const handleImportVideo = async () => {
    try {
      const selected = await openSinglePath(
        [
          {
            name: debugModeEnabled ? 'Video or Image' : 'Video',
            extensions: debugModeEnabled ? ['mp4', 'mov', 'mkv', 'png', 'jpg', 'jpeg', 'webp'] : ['mp4', 'mov', 'mkv'],
          },
        ],
        { lastDirectoryKey: 'last-video-import-dir' },
      )
      if (!selected) {
        return
      }

      if (debugModeEnabled && DEBUG_IMAGE_EXTENSIONS.has(pathExtension(selected))) {
        setImportingVideo(true)
        try {
          if (importedVideoPath) {
            await clearPreviewVideo()
          }
          setImportedBackgroundImage(selected)
          onSetBackgroundMode?.('image')
        } finally {
          setImportingVideo(false)
        }
        return
      }

      await importVideoSelection(selected, {
        setImportingVideo,
        setImportedVideo,
        setConfig,
        clearVideoTelemetry,
        onSetBackgroundMode,
      })
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
        setImportingVideo(true)
        try {
          if (importedVideoPath) {
            await clearPreviewVideo()
          }
          setImportedBackgroundImage(path)
          onSetBackgroundMode?.('image')
        } finally {
          setImportingVideo(false)
        }
        return
      }

      await importVideoSelection(selection, {
        setImportingVideo,
        setImportedVideo,
        setConfig,
        clearVideoTelemetry,
        onSetBackgroundMode,
      })
    } catch (err) {
      console.error('Video drop failed:', err)
      setErrorMessage(`Video drop failed: ${err.message}`)
    } finally {
      setImportingVideo(false)
    }
  }

  const handleClearImportedVideo = async () => {
    try {
      if (importedVideoPath) {
        await clearPreviewVideo()
      }
    } catch (err) {
      console.error('Failed to clear preview video:', err)
    } finally {
      clearImportedVideo()
      onSetBackgroundMode?.('checker')
    }
  }

  const videoSummary = {
    path: importedVideoPath,
    filename: importedVideoFilename,
    duration: importedVideoDuration,
    fps: importedVideoFps,
    resolution: importedVideoResolution,
    creationTime: importedVideoCreationTime,
    timeSource: importedVideoTimeSource,
    codecName: importedVideoCodecName,
    codecLongName: importedVideoCodecLongName,
    bitRate: importedVideoBitRate,
    cameraType: importedVideoCameraType,
    cameraModel: importedVideoCameraModel,
  }

  return {
    debugModeEnabled,
    importedBackgroundImageFilename,
    importedMediaFilename,
    importedVideoFilename,
    videoSummary,
    clearImportedVideo: handleClearImportedVideo,
    handleImportVideo,
    handleVideoFilesDrop,
  }
}
