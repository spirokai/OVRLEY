/**
 * Composes preview-video source resolution, warning lifecycle, scrub
 * scheduling, and playback synchronization around the active <video> element.
 */

import { useCallback, useEffect, useMemo, useRef } from 'react'
import { openHevcSupport } from '@/api/backend'
import useStore from '@/store/useStore'
import { useVideoPlaybackClock } from './useVideoPlaybackClock'
import { useVideoPreviewWarnings } from './useVideoPreviewWarnings'
import { SCRUB_SEEK_EPSILON_SECONDS, SCRUB_SEEK_INTERVAL_MS } from '../data/videoPreviewConstants'
import { primeVideoFirstFrame, syncVideoCurrentTime } from '../utils/videoPreviewPlayback'
import { createVideoPreviewScrubScheduler } from '../utils/videoPreviewScrubScheduler'
import { isVideoPreviewOutOfRange, resolveVideoPreviewSource } from '../utils/videoPreviewSource'
import i18next from 'i18next'

/**
 * Manages the video preview element and synchronization with the global playhead.
 *
 * @param {React.RefObject<HTMLVideoElement>} videoRef - Ref to the video element.
 * @param {boolean} isActive - Whether the imported video preview is currently visible.
 * @returns {{ videoSrc: string, importId: string|null, frozenFrameSecond: number|null, isOutOfRange: boolean, openVideoPreviewHelp: Function, videoPreviewHelpAvailable: boolean, videoPreviewMessages: string[] }}
 */
export function useVideoPreview(videoRef, isActive = true) {
  // Store selectors - subscribes to video import state, playhead position, playback mode, and sync offset.
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoImportId = useStore((state) => state.importedVideoImportId)
  const importedVideoPreviewUrl = useStore((state) => state.importedVideoPreviewUrl)
  const importedVideoPreviewWarnings = useStore((state) => state.importedVideoPreviewWarnings)
  const importedVideoCodecName = useStore((state) => state.importedVideoCodecName)
  const platformOs = useStore((state) => state.platformOs)
  const videoSyncOffsetSeconds = useStore((state) => state.videoSyncOffsetSeconds)
  const videoSyncOffsetPreviewSeconds = useStore((state) => state.videoSyncOffsetPreviewSeconds)
  const selectedSecond = useStore((state) => state.selectedSecond)
  const previewPlaybackState = useStore((state) => state.previewPlaybackState)
  const previewPlaybackSource = useStore((state) => state.previewPlaybackSource)
  const pausePreviewPlayback = useStore((state) => state.pausePreviewPlayback)
  const setSelectedSecond = useStore((state) => state.setSelectedSecond)
  const videoDuration = useStore((state) => state.importedVideoDuration || 0)
  const effectiveVideoSyncOffsetSeconds = videoSyncOffsetPreviewSeconds ?? videoSyncOffsetSeconds

  // Derived state - determines whether the video should play and which source URL to load.
  const isVideoPlaybackMode =
    isActive && previewPlaybackState === 'playing' && previewPlaybackSource === 'video' && videoSyncOffsetPreviewSeconds === null
  const videoSrc = useMemo(
    () =>
      resolveVideoPreviewSource({
        importedVideoPath,
        importedVideoPreviewUrl,
      }),
    [importedVideoPath, importedVideoPreviewUrl],
  )

  // Warning lifecycle - tracks metadata, native-player, and slow-seek messages separately from playback sync.
  const { hevcPlaybackWarning, metadataStatusMessage, nativeVideoError, seekWarning } = useVideoPreviewWarnings({
    codecName: importedVideoCodecName,
    isActive,
    videoRef,
    videoSrc,
  })

  const openVideoPreviewHelp = useCallback(() => {
    openHevcSupport().catch((error) => {
      console.error('[useVideoPreview] Failed to open HEVC help', error)
    })
  }, [])

  // Video playback clock - publishes preview time from the video element while playing.
  useVideoPlaybackClock({
    videoRef,
    isActive: Boolean(videoSrc) && isVideoPlaybackMode,
    videoSyncOffsetSeconds,
    onPreviewSecond: setSelectedSecond,
  })

  const scrubSchedulerRef = useRef(null)
  const videoPlaybackOwnerRef = useRef(null)

  // Drag preview - seek the video directly from the transient offset without committing global sync state.
  useEffect(() => {
    if (videoSyncOffsetPreviewSeconds === null) {
      return
    }

    const video = videoRef.current
    if (!video || !videoSrc) {
      return
    }

    if (!video.paused) {
      video.pause()
    }

    scrubSchedulerRef.current?.schedule(selectedSecond - videoSyncOffsetPreviewSeconds)
  }, [selectedSecond, videoRef, videoSrc, videoSyncOffsetPreviewSeconds])

  // Scrub scheduler ownership - preserve throttle state across playhead updates.
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
    scrubSchedulerRef.current = scrubScheduler

    return () => {
      scrubScheduler.clear()
      if (scrubSchedulerRef.current === scrubScheduler) {
        scrubSchedulerRef.current = null
      }
    }
  }, [isActive, videoRef, videoSrc])

  // Video sync - the video clock owns playback; external playhead updates own paused and scrubbed seeks.
  useEffect(() => {
    const video = videoRef.current
    if (!video || !videoSrc) {
      return undefined
    }

    const syncPlaybackState = () => {
      if (videoSyncOffsetPreviewSeconds !== null) {
        videoPlaybackOwnerRef.current = null
        return
      }

      const desiredVideoSecond = selectedSecond - videoSyncOffsetSeconds

      const handlePlaybackFailure = (error) => {
        const currentState = useStore.getState()
        const isCurrentVideoPlayback =
          videoRef.current === video && currentState.previewPlaybackState === 'playing' && currentState.previewPlaybackSource === 'video'

        if (!isCurrentVideoPlayback) {
          return
        }

        if (error?.name !== i18next.t('video-preview.aborterror', 'AbortError') && error?.name !== 'NotAllowedError') {
          console.error('[useVideoPreview] Failed to start playback', error)
        }

        pausePreviewPlayback(currentState.selectedSecond)
      }

      const startVideoPlayback = () => {
        const playPromise = video.play()

        if (playPromise && typeof playPromise.catch === 'function') {
          playPromise.catch(handlePlaybackFailure)
        }
      }

      if (isVideoPlaybackMode) {
        scrubSchedulerRef.current?.clear()
        const owner = videoPlaybackOwnerRef.current
        if (owner?.video !== video || owner.videoSyncOffsetSeconds !== videoSyncOffsetSeconds) {
          syncVideoCurrentTime(video, desiredVideoSecond)
          videoPlaybackOwnerRef.current = { video, videoSyncOffsetSeconds }
        }

        if (video.paused) {
          startVideoPlayback()
        }

        return
      }

      videoPlaybackOwnerRef.current = null
      if (!video.paused) {
        video.pause()
      }

      if (previewPlaybackState === 'scrubbing') {
        scrubSchedulerRef.current?.schedule(desiredVideoSecond)
        return
      }

      scrubSchedulerRef.current?.clear()
      syncVideoCurrentTime(video, desiredVideoSecond)
    }

    const handleLoadedMetadata = () => {
      syncPlaybackState()
      primeVideoFirstFrame(video)
    }

    syncPlaybackState()
    video.addEventListener('loadedmetadata', handleLoadedMetadata)

    return () => {
      video.removeEventListener('loadedmetadata', handleLoadedMetadata)
    }
  }, [
    isActive,
    isVideoPlaybackMode,
    pausePreviewPlayback,
    previewPlaybackState,
    selectedSecond,
    videoRef,
    videoSrc,
    videoSyncOffsetPreviewSeconds,
    videoSyncOffsetSeconds,
  ])

  // Derived return values - aggregate imported-video warnings with local preview messages.
  const isOutOfRange = isVideoPreviewOutOfRange({
    selectedSecond,
    videoDuration,
    videoSyncOffsetSeconds: effectiveVideoSyncOffsetSeconds,
  })
  const videoEndSecond = effectiveVideoSyncOffsetSeconds + videoDuration
  const frozenFrameSecond = videoSrc && videoDuration > 0 && selectedSecond >= videoEndSecond ? videoDuration : null
  const videoPreviewHelpAvailable = Boolean(hevcPlaybackWarning) && platformOs === 'windows'
  const videoPreviewMessages = [hevcPlaybackWarning, ...importedVideoPreviewWarnings, metadataStatusMessage, seekWarning, nativeVideoError].filter(
    Boolean,
  )

  return {
    videoSrc,
    importId: importedVideoImportId,
    frozenFrameSecond,
    isOutOfRange,
    hevcPlaybackWarning,
    openVideoPreviewHelp,
    videoPreviewHelpAvailable,
    videoPreviewMessages,
  }
}
