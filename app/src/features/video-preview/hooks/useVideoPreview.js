/**
 * Composes preview-video source resolution, warning lifecycle, scrub
 * scheduling, and playback synchronization around the active <video> element.
 */

import { useEffect, useMemo } from 'react'
import useStore from '@/store/useStore'
import { useVideoPlaybackClock } from './useVideoPlaybackClock'
import { useVideoPreviewWarnings } from './useVideoPreviewWarnings'
import { DRIFT_CORRECTION_SECONDS, SCRUB_SEEK_EPSILON_SECONDS, SCRUB_SEEK_INTERVAL_MS } from '../data/videoPreviewConstants'
import { clampVideoTime, syncVideoCurrentTime } from '../utils/videoPreviewPlayback'
import { createVideoPreviewScrubScheduler } from '../utils/videoPreviewScrubScheduler'
import { isVideoPreviewOutOfRange, resolveVideoPreviewSource } from '../utils/videoPreviewSource'

/**
 * Manages the video preview element and synchronization with the global playhead.
 *
 * @param {React.RefObject<HTMLVideoElement>} videoRef - Ref to the video element.
 * @param {boolean} isActive - Whether the imported video preview is currently visible.
 * @returns {{ videoSrc: string, importId: string|null, isOutOfRange: boolean, videoPreviewMessages: string[] }}
 */
export function useVideoPreview(videoRef, isActive = true) {
  // Store selectors - subscribes to video import state, playhead position, playback mode, and sync offset.
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoImportId = useStore((state) => state.importedVideoImportId)
  const importedVideoPreviewUrl = useStore((state) => state.importedVideoPreviewUrl)
  const importedVideoPreviewWarnings = useStore((state) => state.importedVideoPreviewWarnings)
  const videoSyncOffsetSeconds = useStore((state) => state.videoSyncOffsetSeconds)
  const selectedSecond = useStore((state) => state.selectedSecond)
  const previewPlaybackState = useStore((state) => state.previewPlaybackState)
  const previewPlaybackSource = useStore((state) => state.previewPlaybackSource)
  const setSelectedSecond = useStore((state) => state.setSelectedSecond)
  const setImportedVideoPreviewError = useStore((state) => state.setImportedVideoPreviewError)
  const videoDuration = useStore((state) => state.importedVideoDuration || 0)

  // Derived state - determines whether the video should play and which source URL to load.
  const isVideoPlaybackMode = isActive && previewPlaybackState === 'playing' && previewPlaybackSource === 'video'
  const videoSrc = useMemo(
    () =>
      resolveVideoPreviewSource({
        importedVideoPath,
        importedVideoPreviewUrl,
      }),
    [importedVideoPath, importedVideoPreviewUrl],
  )

  // Warning lifecycle - tracks metadata, native-player, and slow-seek messages separately from playback sync.
  const { metadataStatusMessage, nativeVideoError, seekWarning } = useVideoPreviewWarnings({
    setImportedVideoPreviewError,
    videoRef,
    videoSrc,
  })

  // Video playback clock - publishes preview time from the video element while playing.
  useVideoPlaybackClock({
    videoRef,
    isActive: Boolean(videoSrc) && isVideoPlaybackMode,
    videoSyncOffsetSeconds,
    onPreviewSecond: setSelectedSecond,
  })

  // Video sync - keeps play/pause/scrub ownership focused on the active video element.
  useEffect(() => {
    const video = videoRef.current
    if (!video || !videoSrc) {
      return undefined
    }

    const scrubScheduler = createVideoPreviewScrubScheduler({
      epsilonSeconds: SCRUB_SEEK_EPSILON_SECONDS,
      flushIntervalMs: SCRUB_SEEK_INTERVAL_MS,
      video,
    })

    const syncPlaybackState = () => {
      const desiredVideoSecond = selectedSecond - videoSyncOffsetSeconds

      if (isVideoPlaybackMode) {
        scrubScheduler.clear()

        if (Math.abs(clampVideoTime(video, desiredVideoSecond) - video.currentTime) > DRIFT_CORRECTION_SECONDS) {
          syncVideoCurrentTime(video, desiredVideoSecond)
        }

        if (video.paused) {
          const playPromise = video.play()

          if (playPromise && typeof playPromise.catch === 'function') {
            playPromise.catch((error) => {
              if (error?.name !== 'AbortError' && error?.name !== 'NotAllowedError') {
                console.error('[useVideoPreview] Failed to start playback', error)
              }
            })
          }
        }

        return
      }

      if (!video.paused) {
        video.pause()
      }

      if (previewPlaybackState === 'scrubbing') {
        scrubScheduler.schedule(desiredVideoSecond)
        return
      }

      scrubScheduler.clear()
      syncVideoCurrentTime(video, desiredVideoSecond)
    }

    const handleLoadedMetadata = () => {
      syncPlaybackState()
    }

    syncPlaybackState()
    video.addEventListener('loadedmetadata', handleLoadedMetadata)

    return () => {
      video.removeEventListener('loadedmetadata', handleLoadedMetadata)
      scrubScheduler.clear()
    }
  }, [isActive, isVideoPlaybackMode, previewPlaybackState, selectedSecond, videoRef, videoSrc, videoSyncOffsetSeconds])

  // Derived return values - aggregate imported-video warnings with local preview messages.
  const isOutOfRange = isVideoPreviewOutOfRange({
    selectedSecond,
    videoDuration,
    videoSyncOffsetSeconds,
  })
  const videoPreviewMessages = [...importedVideoPreviewWarnings, metadataStatusMessage, seekWarning, nativeVideoError].filter(Boolean)

  return {
    videoSrc,
    importId: importedVideoImportId,
    isOutOfRange,
    videoPreviewMessages,
  }
}
