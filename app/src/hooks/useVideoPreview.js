import { useEffect, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import useStore from '@/store/useStore'
import {
  incrementPreviewPerfCounter,
  previewPerfCounterName,
} from '@/lib/previewPerf'
import { useVideoPlaybackClock } from './useVideoPlaybackClock'

const DRIFT_CORRECTION_SECONDS = 0.25

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

function syncVideoCurrentTime(video, second) {
  const safeSecond = clampVideoTime(video, second)

  if (Math.abs(video.currentTime - safeSecond) <= 0.001) {
    return
  }

  incrementPreviewPerfCounter(
    previewPerfCounterName('video.currentTime assignments'),
  )
  video.currentTime = safeSecond
}

/**
 * Manages the video preview element and synchronization with the global playhead.
 *
 * @param {React.RefObject<HTMLVideoElement>} videoRef Ref to the video element
 * @param {boolean} isActive Whether the imported video preview is currently visible
 */
export function useVideoPreview(videoRef, isActive = true) {
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const videoSyncOffsetSeconds = useStore(
    (state) => state.videoSyncOffsetSeconds,
  )
  const selectedSecond = useStore((state) => state.selectedSecond)
  const previewPlaybackState = useStore((state) => state.previewPlaybackState)
  const previewPlaybackSource = useStore((state) => state.previewPlaybackSource)
  const setSelectedSecondTransient = useStore(
    (state) => state.setSelectedSecondTransient,
  )
  const videoDuration = useStore((state) => state.importedVideoDuration || 0)

  const [videoSrc, setVideoSrc] = useState('')

  const isVideoPlaybackMode =
    isActive &&
    previewPlaybackState === 'playing' &&
    previewPlaybackSource === 'video'

  useVideoPlaybackClock({
    videoRef,
    isActive: Boolean(videoSrc) && isVideoPlaybackMode,
    videoSyncOffsetSeconds,
    onPreviewSecond: setSelectedSecondTransient,
  })

  // Handle source changes
  useEffect(() => {
    if (importedVideoPath) {
      setVideoSrc(convertFileSrc(importedVideoPath))
    } else {
      setVideoSrc('')
    }
  }, [importedVideoPath])

  useEffect(() => {
    const video = videoRef.current
    if (!video || !videoSrc) return

    const syncPlaybackState = () => {
      const desiredVideoSecond = selectedSecond - videoSyncOffsetSeconds

      if (isVideoPlaybackMode) {
        if (
          Math.abs(
            clampVideoTime(video, desiredVideoSecond) - video.currentTime,
          ) > DRIFT_CORRECTION_SECONDS
        ) {
          syncVideoCurrentTime(video, desiredVideoSecond)
        }

        if (video.paused) {
          const playPromise = video.play()
          if (playPromise && typeof playPromise.catch === 'function') {
            playPromise.catch((error) => {
              if (
                error?.name !== 'AbortError' &&
                error?.name !== 'NotAllowedError'
              ) {
                console.error(
                  '[useVideoPreview] Failed to start playback',
                  error,
                )
              }
            })
          }
        }

        return
      }

      if (!video.paused) {
        video.pause()
      }

      syncVideoCurrentTime(video, desiredVideoSecond)
    }

    syncPlaybackState()
    video.addEventListener('loadedmetadata', syncPlaybackState)

    return () => {
      video.removeEventListener('loadedmetadata', syncPlaybackState)
    }
  }, [
    isActive,
    isVideoPlaybackMode,
    previewPlaybackSource,
    previewPlaybackState,
    selectedSecond,
    videoSyncOffsetSeconds,
    videoSrc,
    videoRef,
  ])

  const isOutOfRange =
    selectedSecond < videoSyncOffsetSeconds ||
    selectedSecond > videoSyncOffsetSeconds + videoDuration

  return { videoSrc, isOutOfRange }
}
