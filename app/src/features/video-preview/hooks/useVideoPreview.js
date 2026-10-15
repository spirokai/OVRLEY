/**
 * Composes preview-video source resolution, warning lifecycle, scrub
 * scheduling, and playback synchronization around the active <video> element.
 */

import { useCallback, useEffect, useRef } from 'react'
import { openHevcSupport } from '@/api/backend'
import { useVideoTimeline } from '@/hooks/useVideoTimeline'
import { resolvePreviewTarget } from '@/lib/video-program/playback-target'
import useStore from '@/store/useStore'
import { useVideoPlaybackClock } from './useVideoPlaybackClock'
import { useVideoPreviewWarnings } from './useVideoPreviewWarnings'
import { SCRUB_SEEK_EPSILON_SECONDS, SCRUB_SEEK_INTERVAL_MS } from '../data/videoPreviewConstants'
import { primeVideoFirstFrame, syncVideoCurrentTime } from '../utils/videoPreviewPlayback'
import { createVideoPreviewScrubScheduler } from '../utils/videoPreviewScrubScheduler'
import { resolveVideoPreviewSource } from '../utils/videoPreviewSource'
import i18next from 'i18next'

/**
 * Manages the video preview element and synchronization with the global playhead.
 *
 * @param {React.RefObject<HTMLVideoElement>} videoRef - Ref to the video element.
 * @param {boolean} isActive - Whether the imported video preview is currently visible.
 * @returns {{ videoSrc: string, importId: string|null, isOutOfRange: boolean, openVideoPreviewHelp: Function, videoPreviewHelpAvailable: boolean, videoPreviewMessages: string[] }}
 */
export function useVideoPreview(videoRef, isActive = true) {
  // Store selectors - subscribes to the displayed clips, playhead position, playback mode, and drag state.
  const { clips, program } = useVideoTimeline().displayed
  const isDragPreview = useStore((state) => state.videoDragPreview !== null)
  const platformOs = useStore((state) => state.platformOs)
  const selectedSecond = useStore((state) => state.selectedSecond)
  const previewPlaybackState = useStore((state) => state.previewPlaybackState)
  const previewPlaybackSource = useStore((state) => state.previewPlaybackSource)
  const pausePreviewPlayback = useStore((state) => state.pausePreviewPlayback)
  const setSelectedSecond = useStore((state) => state.setSelectedSecond)
  const startPreviewPlayback = useStore((state) => state.startPreviewPlayback)

  // Derived state - the clip at the playhead (or the upcoming/last clip outside clips) owns the video element.
  const target = resolvePreviewTarget(program, selectedSecond)
  const activeClip = target === null ? null : clips.find((clip) => clip.id === target.videoId)
  const videoOffsetSeconds = activeClip?.offsetSeconds ?? 0
  const videoEndSecond = activeClip === null ? 0 : activeClip.offsetSeconds + activeClip.durationSeconds
  const activePath = activeClip?.path ?? null
  const activePreviewUrl = activeClip?.preview.url ?? null
  const isVideoPlaybackMode = isActive && previewPlaybackState === 'playing' && previewPlaybackSource === 'video' && !isDragPreview
  const videoSrc = activePath === null ? '' : resolveVideoPreviewSource({ importedVideoPath: activePath, importedVideoPreviewUrl: activePreviewUrl })

  // Warning lifecycle - tracks metadata, native-player, and slow-seek messages separately from playback sync.
  const { hevcPlaybackWarning, metadataStatusMessage, nativeVideoError, seekWarning } = useVideoPreviewWarnings({
    codecName: activeClip?.codecName ?? null,
    isActive,
    videoRef,
    videoSrc,
  })

  const openVideoPreviewHelp = useCallback(() => {
    openHevcSupport().catch((error) => {
      console.error('[useVideoPreview] Failed to open HEVC help', error)
    })
  }, [])

  const handoffVideoPlaybackToTimeline = useCallback(() => {
    startPreviewPlayback({ source: 'timeline', second: videoEndSecond })
  }, [startPreviewPlayback, videoEndSecond])

  // Video playback clock - publishes preview time from the video element while playing.
  useVideoPlaybackClock({
    videoRef,
    isActive: Boolean(videoSrc) && isVideoPlaybackMode,
    videoFps: activeClip?.fps ?? null,
    videoOffsetSeconds,
    onPreviewSecond: setSelectedSecond,
    onPlaybackEnded: handoffVideoPlaybackToTimeline,
  })

  const scrubSchedulerRef = useRef(null)
  const videoPlaybackOwnerRef = useRef(null)

  // Drag preview - seek the video directly from the transient offset without committing global sync state.
  useEffect(() => {
    if (!isDragPreview) {
      return
    }

    const video = videoRef.current
    if (!video || !videoSrc) {
      return
    }

    if (!video.paused) {
      video.pause()
    }

    scrubSchedulerRef.current?.schedule(selectedSecond - videoOffsetSeconds)
  }, [isDragPreview, selectedSecond, videoOffsetSeconds, videoRef, videoSrc])

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
      if (isDragPreview) {
        videoPlaybackOwnerRef.current = null
        return
      }

      const desiredVideoSecond = selectedSecond - videoOffsetSeconds

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
        if (owner?.video !== video || owner.videoSrc !== videoSrc || owner.videoOffsetSeconds !== videoOffsetSeconds) {
          syncVideoCurrentTime(video, desiredVideoSecond)
          videoPlaybackOwnerRef.current = { video, videoSrc, videoOffsetSeconds }
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

      if (previewPlaybackState === 'playing') {
        // Timeline-owned playback leaves the frame restored by the video clock untouched.
        scrubSchedulerRef.current?.clear()
        return
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
    isDragPreview,
    isVideoPlaybackMode,
    pausePreviewPlayback,
    previewPlaybackState,
    selectedSecond,
    videoOffsetSeconds,
    videoRef,
    videoSrc,
  ])

  // Derived return values - aggregate imported-video warnings with local preview messages.
  const isOutOfRange = target === null || !target.isInside
  const videoPreviewHelpAvailable = Boolean(hevcPlaybackWarning) && platformOs === 'windows'
  const videoPreviewMessages = [
    hevcPlaybackWarning,
    ...(activeClip?.preview.warnings ?? []),
    metadataStatusMessage,
    seekWarning,
    nativeVideoError,
  ].filter(Boolean)

  return {
    videoSrc,
    importId: activeClip?.preview.importId ?? null,
    isOutOfRange,
    hevcPlaybackWarning,
    openVideoPreviewHelp,
    videoPreviewHelpAvailable,
    videoPreviewMessages,
  }
}
