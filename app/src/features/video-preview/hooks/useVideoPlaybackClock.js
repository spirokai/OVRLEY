/**
 * Publishes preview time from the active <video> element while playback is running.
 *
 * Uses `requestVideoFrameCallback` when available (preferred), falling back to
 * `requestAnimationFrame`. Deduplicates frames by comparing frame index so
 * that the timeline only advances when the video actually produces a new frame.
 */

import { useEffect, useMemo, useRef } from 'react'
import useStore from '@/store/useStore'
import { getEffectivePreviewFps } from '@/features/overlay-editor'
import { incrementPreviewPerfCounter, previewPerfCounterName, setPreviewPerfValue } from '@/lib/previewPerf'
import { PREVIEW_CLOCK_MODE_FLAG } from '../data/videoPreviewConstants'

/**
 * Reads the preview clock mode from localStorage, defaulting to `'auto'`.
 * In `'auto'` mode the hook prefers `requestVideoFrameCallback` when the browser supports it.
 * @returns {'auto'|'raf'} The resolved clock mode.
 */
function resolvePreviewClockMode() {
  if (typeof window === 'undefined') {
    return 'auto'
  }

  try {
    return window.localStorage.getItem(PREVIEW_CLOCK_MODE_FLAG) || 'auto'
  } catch {
    return 'auto'
  }
}

/**
 * Publishes preview time from the active video element while playback is running.
 *
 * @param {object} options - Hook configuration.
 * @param {React.RefObject<HTMLVideoElement>} options.videoRef Ref to preview video.
 * @param {boolean} options.isActive Whether video-clock playback is active.
 * @param {number} options.videoSyncOffsetSeconds Timeline offset for the video.
 * @param {(second: number) => void} options.onPreviewSecond Callback receiving timeline time.
 * @returns {void}
 */
export function useVideoPlaybackClock({ videoRef, isActive, videoSyncOffsetSeconds, onPreviewSecond }) {
  // Store selectors — picks scene FPS, update rate, and imported video FPS from Zustand
  const sceneFps = useStore((state) => state.config?.scene?.fps ?? 30)
  const updateRate = useStore((state) => state.updateRate)
  const importedVideoFps = useStore((state) => state.importedVideoFps)

  // Internal refs — tracks scheduled callback handle, callback type ('video'|'animation'),
  // last published frame index for dedup, and last video source URL to detect source changes
  const callbackIdRef = useRef(null)
  const callbackTypeRef = useRef(null)
  const publishedFrameRef = useRef(-1)
  const lastVideoSourceRef = useRef('')

  // Derived state — computes effective preview FPS from scene FPS / update rate and resolves the clock mode
  const effectivePreviewFps = useMemo(() => Math.max(1, getEffectivePreviewFps(sceneFps, updateRate) || 0), [sceneFps, updateRate])
  const previewClockMode = resolvePreviewClockMode()
  const shouldForceAnimationClock = previewClockMode === 'raf'

  // Performance counters — reports clock metrics to the perf debug panel
  useEffect(() => {
    setPreviewPerfValue('effective preview fps', Math.round(effectivePreviewFps * 100) / 100)
  }, [effectivePreviewFps])

  useEffect(() => {
    setPreviewPerfValue('preview clock mode', previewClockMode)
  }, [previewClockMode])

  useEffect(() => {
    if (!Number.isFinite(importedVideoFps)) {
      return
    }

    setPreviewPerfValue('imported video fps', Math.round(importedVideoFps * 100) / 100)
  }, [importedVideoFps])

  // Frame scheduling — manages video frame callback or rAF loop to publish preview time
  useEffect(() => {
    const video = videoRef.current
    if (!video || !isActive) {
      return undefined
    }

    /** Cancels whichever scheduled frame type is pending ('video' via cancelVideoFrameCallback or 'animation' via cancelAnimationFrame). */
    const cancelScheduledFrame = () => {
      if (callbackIdRef.current === null) {
        return
      }

      if (callbackTypeRef.current === 'video') {
        video.cancelVideoFrameCallback?.(callbackIdRef.current)
      } else if (callbackTypeRef.current === 'animation') {
        window.cancelAnimationFrame(callbackIdRef.current)
      }

      callbackIdRef.current = null
      callbackTypeRef.current = null
    }

    /** Computes the current timeline second from video.currentTime + offset and publishes it via onPreviewSecond if the frame index changed (dedup). */
    const publishPreviewSecond = () => {
      incrementPreviewPerfCounter(previewPerfCounterName('video frame callbacks'))

      const previewSecond = video.currentTime + videoSyncOffsetSeconds
      const nextFrame = Math.floor(previewSecond * effectivePreviewFps)

      if (nextFrame === publishedFrameRef.current) {
        return
      }

      publishedFrameRef.current = nextFrame
      onPreviewSecond(previewSecond)
    }

    /** Publishes the final timeline second when the video ends (one frame past the end to signal completion). */
    const publishVideoEndSecond = () => {
      incrementPreviewPerfCounter(previewPerfCounterName('video frame callbacks'))

      const safeDuration = Number(video.duration)
      const finalVideoSecond = Number.isFinite(safeDuration) ? safeDuration : video.currentTime
      const previewSecond = finalVideoSecond + videoSyncOffsetSeconds + 1 / effectivePreviewFps

      publishedFrameRef.current = -1
      onPreviewSecond(previewSecond)
    }

    /** Resets the frame dedup counter when the video source URL changes. */
    const syncVideoSource = () => {
      const currentSource = video.currentSrc || video.src || ''
      if (currentSource === lastVideoSourceRef.current) {
        return
      }

      lastVideoSourceRef.current = currentSource
      publishedFrameRef.current = -1
    }

    /** Schedules the next frame callback — prefers requestVideoFrameCallback, falls back to requestAnimationFrame. */
    const scheduleNextFrame = () => {
      if (!isActive || video.paused || video.ended) {
        cancelScheduledFrame()
        return
      }

      if (!shouldForceAnimationClock && typeof video.requestVideoFrameCallback === 'function') {
        callbackTypeRef.current = 'video'
        callbackIdRef.current = video.requestVideoFrameCallback(() => {
          syncVideoSource()
          publishPreviewSecond()
          scheduleNextFrame()
        })
        return
      }

      callbackTypeRef.current = 'animation'
      callbackIdRef.current = window.requestAnimationFrame(() => {
        syncVideoSource()
        publishPreviewSecond()
        scheduleNextFrame()
      })
    }

    // Event handlers — respond to video play/pause/ended/src changes
    const handlePlaybackStart = () => {
      syncVideoSource()
      publishPreviewSecond()
      scheduleNextFrame()
    }

    const handlePlaybackStop = () => {
      cancelScheduledFrame()
    }

    const handlePlaybackEnded = () => {
      publishVideoEndSecond()
      cancelScheduledFrame()
    }

    const handleSourceChange = () => {
      syncVideoSource()
      if (isActive && !video.paused && !video.ended) {
        handlePlaybackStart()
      }
    }

    // Initialise — if the video is already playing, start the clock immediately
    syncVideoSource()

    if (!video.paused && !video.ended) {
      handlePlaybackStart()
    }

    // Event binding — registers and cleans up video element listeners
    video.addEventListener('play', handlePlaybackStart)
    video.addEventListener('pause', handlePlaybackStop)
    video.addEventListener('ended', handlePlaybackEnded)
    video.addEventListener('emptied', handleSourceChange)
    video.addEventListener('loadedmetadata', handleSourceChange)

    return () => {
      video.removeEventListener('play', handlePlaybackStart)
      video.removeEventListener('pause', handlePlaybackStop)
      video.removeEventListener('ended', handlePlaybackEnded)
      video.removeEventListener('emptied', handleSourceChange)
      video.removeEventListener('loadedmetadata', handleSourceChange)
      cancelScheduledFrame()
      publishedFrameRef.current = -1
    }
  }, [effectivePreviewFps, isActive, onPreviewSecond, shouldForceAnimationClock, videoRef, videoSyncOffsetSeconds])
}
