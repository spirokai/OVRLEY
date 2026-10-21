/**
 * Video import - background media selection and preview management. Selected
 * videos are reviewed in the import dialog before they join the collection.
 */

import { useMemo } from 'react'
import { clearPreviewVideos, removePreviewVideo } from '@/api/backend'
import { changeVideoCollection } from '@/features/undo-redo/undoHistory'
import { openMultiplePaths } from '@/lib/file-dialog'
import { filenameFromSelectedPath } from '@/lib/utils'
import { joinVideoEdits, sortVideosByTimeline } from '@/lib/video-program/video-collection'
import useStore from '@/store/useStore'
import i18next from 'i18next'
import useVideoImportReview from './useVideoImportReview'

const DEBUG_IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'webp'])

function pathExtension(path) {
  return typeof path === 'string' ? path.split('.').pop()?.toLowerCase() || '' : ''
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
    filename: filenameFromSelectedPath(video.path),
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
    offsetSeconds: video.offsetSeconds,
    telemetryStatus: video.telemetry.status,
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
  const importedBackgroundImageFilename = importedBackgroundImagePath ? filenameFromSelectedPath(importedBackgroundImagePath) : null
  const importedMediaFilename = importedBackgroundImageFilename || importedVideoLabel

  const review = useVideoImportReview({ onImported: (videos) => videos.length > 0 && onSetBackgroundMode?.('video') })

  const applyBackgroundImage = async (path) => {
    setImportingVideo(true)
    try {
      if (hasVideos) {
        await clearPreviewVideos()
        useStore.getState().clearTransitionProxies()
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

      await review.start(selected)
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

      await review.start(selections)
    } catch (err) {
      console.error('Video drop failed:', err)
      setErrorMessage(`Video drop failed: ${err.message}`)
    }
  }

  /**
   * Removes one clip from the collection, then releases its preview registration.
   * @param {string} videoId Clip to remove.
   */
  const removeVideo = async (videoId) => {
    try {
      const state = useStore.getState()
      const { importId } = state.importedVideos.find((video) => video.id === videoId)?.preview ?? {}
      // Playback stops before the clip leaves; the removal clamps the playhead.
      if (state.previewPlaybackState === 'playing') state.pausePreviewPlayback(state.selectedSecond)
      changeVideoCollection(useStore, () => useStore.getState().removeImportedVideo(videoId))
      await removePreviewVideo(importId)
      if (useStore.getState().importedVideos.length === 0) onSetBackgroundMode?.('checker')
    } catch (err) {
      console.error('Failed to remove video:', err)
      setErrorMessage(`Removing the video failed: ${err.message}`)
    }
  }

  const handleClearImportedVideos = async () => {
    try {
      if (hasVideos) {
        await clearPreviewVideos()
        useStore.getState().clearTransitionProxies()
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
    importDialog: review.dialog,
    removeVideo,
  }
}
