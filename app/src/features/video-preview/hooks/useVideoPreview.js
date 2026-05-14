/**
 * Manages the <video> preview element: source resolution, metadata loading,
 * seek/scrub throttling, playback sync, drift correction, and user-facing
 * warnings for slow loading or slow seeking.
 */

import { useEffect, useRef, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import useStore from '@/store/useStore'
import { incrementPreviewPerfCounter, previewPerfCounterName } from '@/lib/previewPerf'
import { useVideoPlaybackClock } from './useVideoPlaybackClock'
import {
  DRIFT_CORRECTION_SECONDS,
  METADATA_SOFT_WARNING_MS,
  METADATA_STRONG_WARNING_MS,
  SCRUB_SEEK_EPSILON_SECONDS,
  SCRUB_SEEK_INTERVAL_MS,
  SLOW_SEEK_WARNING_COUNT,
  SLOW_SEEK_WARNING_MS,
  USE_LOCAL_HTTP_VIDEO_PREVIEW,
} from '../data/videoPreviewConstants'

/**
 * Clamps a time value to the video's valid range [0, duration].
 * Returns 0 for negative, NaN, or non-finite values.
 * @param {HTMLVideoElement} video - The video element whose duration is used as upper bound.
 * @param {number} second - Requested time in seconds.
 * @returns {number} Clamped time in seconds within [0, duration].
 */
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

/**
 * Assigns `video.currentTime` to the given second (clamped) but skips the
 * assignment if the difference is within `epsilonSeconds`. This avoids
 * triggering redundant seek events for negligible positioning changes.
 * @param {HTMLVideoElement} video - The video element to sync.
 * @param {number} second - Target time in seconds.
 * @param {number} [epsilonSeconds=0.001] - Tolerance below which the assignment is skipped.
 */
function syncVideoCurrentTime(video, second, epsilonSeconds = 0.001) {
  const safeSecond = clampVideoTime(video, second)

  if (Math.abs(video.currentTime - safeSecond) <= epsilonSeconds) {
    return
  }

  incrementPreviewPerfCounter(previewPerfCounterName('video.currentTime assignments'))
  video.currentTime = safeSecond
}

/**
 * Produces a human-readable description for a `MediaError` based on its `.code`.
 * @param {MediaError|null|undefined} error - The error object from the video element.
 * @returns {string} A user-facing error description string.
 */
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
 * @returns {{ videoSrc: string, importId: string|null, isOutOfRange: boolean, videoPreviewMessages: string[] }}
 */
export function useVideoPreview(videoRef, isActive = true) {
  // Store selectors — subscribes to video import state, playhead position, playback mode, and sync offset
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

  // Local UI state — video source URL and user-facing warning messages
  const [videoSrc, setVideoSrc] = useState('')
  const [metadataStatusMessage, setMetadataStatusMessage] = useState('')
  const [nativeVideoError, setNativeVideoError] = useState('')
  const [seekWarning, setSeekWarning] = useState('')

  // Internal refs — scrub throttling timers, metadata loading timers, slow-seek detection
  const pendingScrubSecondRef = useRef(null)
  const pendingScrubTimeoutRef = useRef(null)
  const lastScrubSeekMsRef = useRef(0)
  const metadataSoftTimerRef = useRef(null)
  const metadataStrongTimerRef = useRef(null)
  const seekStartedAtMsRef = useRef(null)
  const slowSeekCountRef = useRef(0)

  // Derived state — determines whether the video should play (vs. pause/scrub/static)
  const isVideoPlaybackMode = isActive && previewPlaybackState === 'playing' && previewPlaybackSource === 'video'

  // Video playback clock — publishes preview time from the video element while playing,
  // driving the global timeline via setSelectedSecondTransient to keep the playhead in sync
  useVideoPlaybackClock({
    videoRef,
    isActive: Boolean(videoSrc) && isVideoPlaybackMode,
    videoSyncOffsetSeconds,
    onPreviewSecond: setSelectedSecondTransient,
  })

  // Video source — resolves the preview URL from the store path or HTTP preview endpoint
  useEffect(() => {
    if (USE_LOCAL_HTTP_VIDEO_PREVIEW && importedVideoPreviewUrl) {
      setVideoSrc(importedVideoPreviewUrl)
    } else if (importedVideoPath) {
      setVideoSrc(convertFileSrc(importedVideoPath))
    } else {
      setVideoSrc('')
    }
  }, [importedVideoPath, importedVideoPreviewUrl])

  // Metadata timers — shows progressive warnings (soft → strong) when the video's metadata
  // takes unusually long to load, and clears all state+timers when videoSrc changes
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

  // Scrub cleanup — clears any pending scrub seek timer and refs when the video source changes,
  // preventing stale seeks from executing against a now-incorrect source
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

  // Video sync — primary effect that manages play/pause/scrub state, drift correction,
  // and event handlers for loadedmetadata, error, seeking, and seeked
  useEffect(() => {
    const video = videoRef.current
    if (!video || !videoSrc) return

    // Scrub scheduling — clears any pending or queued seek
    const clearPendingScrubSeek = () => {
      if (pendingScrubTimeoutRef.current) {
        window.clearTimeout(pendingScrubTimeoutRef.current)
        pendingScrubTimeoutRef.current = null
      }
      pendingScrubSecondRef.current = null
    }

    // Scrub scheduling — immediately executes the most recently requested scrub seek,
    // bypassing the epsilon check so the seek always fires
    const flushPendingScrubSeek = () => {
      pendingScrubTimeoutRef.current = null
      const nextSecond = pendingScrubSecondRef.current
      pendingScrubSecondRef.current = null
      lastScrubSeekMsRef.current = performance.now()

      if (nextSecond !== null) {
        syncVideoCurrentTime(video, nextSecond, SCRUB_SEEK_EPSILON_SECONDS)
      }
    }

    // Scrub scheduling — coalesces rapid scrub requests: stores the latest second,
    // and either flushes immediately (if enough time has elapsed) or schedules a
    // deferred flush for the remaining interval
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

    // Playback state machine — syncs the video element with the global playhead:
    // - In playback mode: plays the video and applies drift correction if the playhead drifts too far
    // - In scrubbing mode: throttles seeks via scheduleScrubSeek
    // - Otherwise (paused/idle): directly seeks to the desired second
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

    // Video event handlers

    /** Clears metadata loading timers and immediately syncs playback state once metadata is ready. */
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

    /** Translates the native MediaError into a user-facing message and sets it in both local state and the store. */
    const handleVideoError = () => {
      const message = describeMediaError(video.error)
      setNativeVideoError(message)
      setImportedVideoPreviewError(message)
    }

    /** Records the start time of a seek operation for slow-seek latency detection. */
    const handleSeeking = () => {
      seekStartedAtMsRef.current = performance.now()
    }

    /** Measures seek latency and increments/decrements a counter; shows a warning if several consecutive slow seeks are detected. */
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

    // Initialise — perform initial sync, then bind event listeners
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

  // Derived return values — computes whether the playhead is outside the video range
  // and aggregates all preview-related messages into a single array
  const isOutOfRange = selectedSecond < videoSyncOffsetSeconds || selectedSecond > videoSyncOffsetSeconds + videoDuration
  const videoPreviewMessages = [...importedVideoPreviewWarnings, metadataStatusMessage, seekWarning, nativeVideoError].filter(Boolean)

  return { videoSrc, importId: importedVideoImportId, isOutOfRange, videoPreviewMessages }
}
