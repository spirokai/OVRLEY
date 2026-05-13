import { useEffect, useRef, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import useStore from '@/store/useStore'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/previewPerf'
import { useVideoPlaybackClock } from './useVideoPlaybackClock'

const DRIFT_CORRECTION_SECONDS = 0.25
const SCRUB_SEEK_INTERVAL_MS = 50
const SCRUB_SEEK_EPSILON_SECONDS = 0.05
const METADATA_SOFT_WARNING_MS = 10_000
const METADATA_STRONG_WARNING_MS = 35_000
const SLOW_SEEK_WARNING_MS = 1_200
const SLOW_SEEK_WARNING_COUNT = 2
const USE_LOCAL_HTTP_VIDEO_PREVIEW = import.meta.env.VITE_USE_LOCAL_HTTP_VIDEO_PREVIEW !== 'false'

function clampVideoTime(video, second) {
  const nextSecond = Number(second)

  if (!Number.isFinite(nextSecond) || nextSecond <= 0) {
    return 0
  }

  const duration = Number(video.duration)
  if (Number.isFinite(duration) && duration > 0) {
    return Math.min(nextSecond, duration)
  }

  return nextSecond
}

function syncVideoCurrentTime(video, second, epsilonSeconds = 0.001) {
  const safeSecond = clampVideoTime(video, second)

  if (Math.abs(video.currentTime - safeSecond) <= epsilonSeconds) {
    return
  }

  incrementPreviewPerfCounter(previewPerfCounterName('video.currentTime assignments'))
  video.currentTime = safeSecond
}

function describeMediaError(error) {
  switch (error?.code) {
    case MediaError.MEDIA_ERR_ABORTED:
      return 'Video preview loading was aborted.'
    case MediaError.MEDIA_ERR_NETWORK:
      return 'The local preview server could not read the video file. The file may have been moved, deleted, or become unavailable.'
    case MediaError.MEDIA_ERR_DECODE:
      return 'The video could not be decoded by the system video player. This may happen with some HEVC, 10-bit, or 4:2:2 files.'
    case MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED:
      return 'This video format is not supported by the system video player.'
    default:
      return 'The video preview could not be loaded.'
  }
}

/**
 * Manages the video preview element and synchronization with the global playhead.
 *
 * @param {React.RefObject<HTMLVideoElement>} videoRef Ref to the video element
 * @param {boolean} isActive Whether the imported video preview is currently visible
 */
export function useVideoPreview(videoRef, isActive = true) {
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const importedVideoImportId = useStore((state) => state.importedVideoImportId)
  const importedVideoPreviewUrl = useStore((state) => state.importedVideoPreviewUrl)
  const importedVideoPreviewWarnings = useStore((state) => state.importedVideoPreviewWarnings)
  const videoSyncOffsetSeconds = useStore((state) => state.videoSyncOffsetSeconds)
  const selectedSecond = useStore((state) => state.selectedSecond)
  const previewPlaybackState = useStore((state) => state.previewPlaybackState)
  const previewPlaybackSource = useStore((state) => state.previewPlaybackSource)
  const setSelectedSecondTransient = useStore((state) => state.setSelectedSecondTransient)
  const setImportedVideoPreviewError = useStore((state) => state.setImportedVideoPreviewError)
  const videoDuration = useStore((state) => state.importedVideoDuration || 0)

  const [videoSrc, setVideoSrc] = useState('')
  const [metadataStatusMessage, setMetadataStatusMessage] = useState('')
  const [nativeVideoError, setNativeVideoError] = useState('')
  const [seekWarning, setSeekWarning] = useState('')
  const pendingScrubSecondRef = useRef(null)
  const pendingScrubTimeoutRef = useRef(null)
  const lastScrubSeekMsRef = useRef(0)
  const metadataSoftTimerRef = useRef(null)
  const metadataStrongTimerRef = useRef(null)
  const seekStartedAtMsRef = useRef(null)
  const slowSeekCountRef = useRef(0)

  const isVideoPlaybackMode = isActive && previewPlaybackState === 'playing' && previewPlaybackSource === 'video'

  useVideoPlaybackClock({
    videoRef,
    isActive: Boolean(videoSrc) && isVideoPlaybackMode,
    videoSyncOffsetSeconds,
    onPreviewSecond: setSelectedSecondTransient,
  })

  // Handle source changes
  useEffect(() => {
    if (USE_LOCAL_HTTP_VIDEO_PREVIEW && importedVideoPreviewUrl) {
      setVideoSrc(importedVideoPreviewUrl)
    } else if (importedVideoPath) {
      setVideoSrc(convertFileSrc(importedVideoPath))
    } else {
      setVideoSrc('')
    }
  }, [importedVideoPath, importedVideoPreviewUrl])

  useEffect(() => {
    const clearMetadataTimers = () => {
      if (metadataSoftTimerRef.current) {
        window.clearTimeout(metadataSoftTimerRef.current)
        metadataSoftTimerRef.current = null
      }
      if (metadataStrongTimerRef.current) {
        window.clearTimeout(metadataStrongTimerRef.current)
        metadataStrongTimerRef.current = null
      }
    }

    clearMetadataTimers()
    setMetadataStatusMessage('')
    setNativeVideoError('')
    setSeekWarning('')
    setImportedVideoPreviewError(null)
    slowSeekCountRef.current = 0
    seekStartedAtMsRef.current = null

    if (!videoSrc) {
      return undefined
    }

    metadataSoftTimerRef.current = window.setTimeout(() => {
      setMetadataStatusMessage('Loading video metadata...')
    }, METADATA_SOFT_WARNING_MS)
    metadataStrongTimerRef.current = window.setTimeout(() => {
      setMetadataStatusMessage('This file is taking unusually long to load. It may be on a slow drive or use metadata stored at the end of the file.')
    }, METADATA_STRONG_WARNING_MS)

    return clearMetadataTimers
  }, [setImportedVideoPreviewError, videoSrc])

  useEffect(
    () => () => {
      if (pendingScrubTimeoutRef.current) {
        window.clearTimeout(pendingScrubTimeoutRef.current)
        pendingScrubTimeoutRef.current = null
      }
      pendingScrubSecondRef.current = null
      lastScrubSeekMsRef.current = 0
    },
    [videoSrc],
  )

  useEffect(() => {
    const video = videoRef.current
    if (!video || !videoSrc) return

    const clearPendingScrubSeek = () => {
      if (pendingScrubTimeoutRef.current) {
        window.clearTimeout(pendingScrubTimeoutRef.current)
        pendingScrubTimeoutRef.current = null
      }
      pendingScrubSecondRef.current = null
    }

    const flushPendingScrubSeek = () => {
      pendingScrubTimeoutRef.current = null
      const nextSecond = pendingScrubSecondRef.current
      pendingScrubSecondRef.current = null
      lastScrubSeekMsRef.current = performance.now()

      if (nextSecond !== null) {
        syncVideoCurrentTime(video, nextSecond, SCRUB_SEEK_EPSILON_SECONDS)
      }
    }

    const scheduleScrubSeek = (second) => {
      pendingScrubSecondRef.current = second
      const now = performance.now()
      const elapsedMs = now - lastScrubSeekMsRef.current

      if (elapsedMs >= SCRUB_SEEK_INTERVAL_MS) {
        if (pendingScrubTimeoutRef.current) {
          window.clearTimeout(pendingScrubTimeoutRef.current)
          pendingScrubTimeoutRef.current = null
        }
        flushPendingScrubSeek()
        return
      }

      if (!pendingScrubTimeoutRef.current) {
        pendingScrubTimeoutRef.current = window.setTimeout(flushPendingScrubSeek, SCRUB_SEEK_INTERVAL_MS - elapsedMs)
      }
    }

    const syncPlaybackState = () => {
      const desiredVideoSecond = selectedSecond - videoSyncOffsetSeconds

      if (isVideoPlaybackMode) {
        clearPendingScrubSeek()

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
        scheduleScrubSeek(desiredVideoSecond)
        return
      }

      clearPendingScrubSeek()
      syncVideoCurrentTime(video, desiredVideoSecond)
    }

    const handleLoadedMetadata = () => {
      if (metadataSoftTimerRef.current) {
        window.clearTimeout(metadataSoftTimerRef.current)
        metadataSoftTimerRef.current = null
      }
      if (metadataStrongTimerRef.current) {
        window.clearTimeout(metadataStrongTimerRef.current)
        metadataStrongTimerRef.current = null
      }
      setMetadataStatusMessage('')
      syncPlaybackState()
    }

    const handleVideoError = () => {
      const message = describeMediaError(video.error)
      setNativeVideoError(message)
      setImportedVideoPreviewError(message)
    }

    const handleSeeking = () => {
      seekStartedAtMsRef.current = performance.now()
    }

    const handleSeeked = () => {
      const startedAt = seekStartedAtMsRef.current
      seekStartedAtMsRef.current = null

      if (startedAt === null) {
        return
      }

      const latencyMs = performance.now() - startedAt
      if (latencyMs >= SLOW_SEEK_WARNING_MS) {
        slowSeekCountRef.current += 1
      } else {
        slowSeekCountRef.current = Math.max(0, slowSeekCountRef.current - 1)
      }

      if (slowSeekCountRef.current >= SLOW_SEEK_WARNING_COUNT) {
        setSeekWarning('Seeking is slow for this file. A lower-resolution preview proxy may improve responsiveness.')
      }
    }

    syncPlaybackState()
    video.addEventListener('loadedmetadata', handleLoadedMetadata)
    video.addEventListener('error', handleVideoError)
    video.addEventListener('seeking', handleSeeking)
    video.addEventListener('seeked', handleSeeked)

    return () => {
      video.removeEventListener('loadedmetadata', handleLoadedMetadata)
      video.removeEventListener('error', handleVideoError)
      video.removeEventListener('seeking', handleSeeking)
      video.removeEventListener('seeked', handleSeeked)
    }
  }, [
    isActive,
    isVideoPlaybackMode,
    previewPlaybackSource,
    previewPlaybackState,
    selectedSecond,
    setImportedVideoPreviewError,
    videoSyncOffsetSeconds,
    videoSrc,
    videoRef,
  ])

  const isOutOfRange = selectedSecond < videoSyncOffsetSeconds || selectedSecond > videoSyncOffsetSeconds + videoDuration
  const videoPreviewMessages = [...importedVideoPreviewWarnings, metadataStatusMessage, seekWarning, nativeVideoError].filter(Boolean)

  return { videoSrc, importId: importedVideoImportId, isOutOfRange, videoPreviewMessages }
}
