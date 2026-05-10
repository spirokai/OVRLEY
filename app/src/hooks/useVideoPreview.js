import { useEffect, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import useStore from '@/store/useStore'

/**
 * Manages the video preview element and synchronization with the global playhead.
 *
 * @param {React.RefObject<HTMLVideoElement>} videoRef Ref to the video element
 */
export function useVideoPreview(videoRef) {
  const importedVideoPath = useStore((state) => state.importedVideoPath)
  const videoSyncOffsetSeconds = useStore(
    (state) => state.videoSyncOffsetSeconds,
  )
  const selectedSecond = useStore((state) => state.selectedSecond)
  const isUpdatingFromTimeline = useStore(
    (state) => state.isUpdatingFromTimeline,
  )

  const [videoSrc, setVideoSrc] = useState('')

  // Handle source changes
  useEffect(() => {
    if (importedVideoPath) {
      setVideoSrc(convertFileSrc(importedVideoPath))
    } else {
      setVideoSrc('')
    }
  }, [importedVideoPath])

  // Handle seeking / synchronization
  useEffect(() => {
    const video = videoRef.current
    if (!video || !videoSrc) return

    const videoTime = selectedSecond - videoSyncOffsetSeconds

    // We only seek if the difference is significant to avoid overhead
    // or if we are dragging the timeline.
    // For 30fps, 1 frame is ~0.033s.
    const diff = Math.abs(video.currentTime - videoTime)

    if (diff > 0.05 || isUpdatingFromTimeline) {
      if (videoTime < 0) {
        video.currentTime = 0
      } else if (video.duration && videoTime > video.duration) {
        video.currentTime = video.duration
      } else {
        video.currentTime = videoTime
      }
    }
  }, [
    selectedSecond,
    videoSyncOffsetSeconds,
    videoSrc,
    isUpdatingFromTimeline,
    videoRef,
  ])

  const videoDuration = useStore((state) => state.importedVideoDuration || 0)
  const isOutOfRange =
    selectedSecond < videoSyncOffsetSeconds ||
    selectedSecond > videoSyncOffsetSeconds + videoDuration

  return { videoSrc, isOutOfRange }
}
