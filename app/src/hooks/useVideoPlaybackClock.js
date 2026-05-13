import { useEffect, useMemo, useRef } from 'react'
import useStore from '@/store/useStore'
import { getEffectivePreviewFps } from '@/components/overlay-editor/previewInterpolation'
import { incrementPreviewPerfCounter, previewPerfCounterName, setPreviewPerfValue } from '@/lib/previewPerf'

const PREVIEW_CLOCK_MODE_FLAG = 'ovrley:preview-clock-mode'

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
 */
export function useVideoPlaybackClock({ videoRef, isActive, videoSyncOffsetSeconds, onPreviewSecond }) {
  const sceneFps = useStore((state) => state.config?.scene?.fps ?? 30)
  const updateRate = useStore((state) => state.updateRate)
  const importedVideoFps = useStore((state) => state.importedVideoFps)
  const callbackIdRef = useRef(null)
  const callbackTypeRef = useRef(null)
  const publishedFrameRef = useRef(-1)
  const lastVideoSourceRef = useRef('')

  const effectivePreviewFps = useMemo(() => Math.max(1, getEffectivePreviewFps(sceneFps, updateRate) || 0), [sceneFps, updateRate])
  const previewClockMode = resolvePreviewClockMode()
  const shouldForceAnimationClock = previewClockMode === 'raf'

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

  useEffect(() => {
    const video = videoRef.current
    if (!video || !isActive) {
      return undefined
    }

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

    const publishVideoEndSecond = () => {
      incrementPreviewPerfCounter(previewPerfCounterName('video frame callbacks'))

      const safeDuration = Number(video.duration)
      const finalVideoSecond = Number.isFinite(safeDuration) ? safeDuration : video.currentTime
      const previewSecond = finalVideoSecond + videoSyncOffsetSeconds + 1 / effectivePreviewFps

      publishedFrameRef.current = -1
      onPreviewSecond(previewSecond)
    }

    const syncVideoSource = () => {
      const currentSource = video.currentSrc || video.src || ''
      if (currentSource === lastVideoSourceRef.current) {
        return
      }

      lastVideoSourceRef.current = currentSource
      publishedFrameRef.current = -1
    }

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

    syncVideoSource()

    if (!video.paused && !video.ended) {
      handlePlaybackStart()
    }

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
