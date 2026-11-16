/**
 * Drives the transition proxy element: parked on its first frame while the outgoing piece plays towards
 * the transition, then kept at the playhead's position in the transition window.
 */

import { useEffect, useState } from 'react'
import { primeVideoFirstFrame, syncVideoCurrentTime } from '../utils/videoPreviewPlayback'

/** Drift tolerated while the proxy plays alongside the outgoing video's clock. */
const PROXY_DRIFT_SECONDS = 0.15
const HAVE_CURRENT_DATA = 2

/**
 * @param {React.RefObject<HTMLVideoElement>} proxyRef Proxy video element.
 * @param {{proxyUrl: string|null, isInWindow: boolean, elapsedSeconds: number, isPlaying: boolean}|null} transition Upcoming transition from `useVideoPreview`.
 * @returns {boolean} Whether the proxy has a decoded frame to show; until then the outgoing video stays visible.
 */
export function useTransitionProxyPlayback(proxyRef, transition) {
  const url = transition?.proxyUrl ?? null
  const isInWindow = transition?.isInWindow ?? false
  const elapsedSeconds = transition?.elapsedSeconds ?? 0
  const isPlaying = transition?.isPlaying ?? false
  const [frameReadyUrl, setFrameReadyUrl] = useState(null)

  // First frame - a freshly loaded proxy is primed into decoding a frame, and is shown only once it has one.
  useEffect(() => {
    const video = proxyRef.current
    if (!video || url === null) return undefined
    const markFrameReady = () => {
      if (video.readyState >= HAVE_CURRENT_DATA) setFrameReadyUrl(url)
    }
    const handleLoadedMetadata = () => primeVideoFirstFrame(video)
    markFrameReady()
    video.addEventListener('loadedmetadata', handleLoadedMetadata)
    video.addEventListener('loadeddata', markFrameReady)
    video.addEventListener('seeked', markFrameReady)
    return () => {
      video.removeEventListener('loadedmetadata', handleLoadedMetadata)
      video.removeEventListener('loadeddata', markFrameReady)
      video.removeEventListener('seeked', markFrameReady)
    }
  }, [proxyRef, url])

  // Position - parked at the window's start before it, at the playhead inside it.
  useEffect(() => {
    const video = proxyRef.current
    if (!video || url === null) return
    if (!isInWindow || !isPlaying) {
      if (!video.paused) video.pause()
      syncVideoCurrentTime(video, elapsedSeconds)
      return
    }
    // A seek still in progress would only be restarted by another one: with GStreamer every seek flushes the
    // pipeline, so correcting before it settles keeps the proxy behind for good.
    if (!video.seeking && Math.abs(video.currentTime - elapsedSeconds) > PROXY_DRIFT_SECONDS) syncVideoCurrentTime(video, elapsedSeconds)
    if (video.paused) {
      video.play()?.catch((error) => {
        if (error?.name !== 'AbortError') console.error('[useTransitionProxyPlayback] Failed to play the transition preview', error)
      })
    }
  }, [elapsedSeconds, isInWindow, isPlaying, proxyRef, url])

  return url !== null && frameReadyUrl === url
}
