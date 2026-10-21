/**
 * Keeps the transition proxy element at the playhead's position in the transition window.
 */

import { useEffect } from 'react'
import { syncVideoCurrentTime } from '../utils/videoPreviewPlayback'

/** Drift tolerated while the proxy plays alongside the outgoing video's clock. */
const PROXY_DRIFT_SECONDS = 0.15

/**
 * @param {React.RefObject<HTMLVideoElement>} proxyRef Proxy video element.
 * @param {{proxyUrl: string|null, elapsedSeconds: number, isPlaying: boolean}|null} transition Transition window from `useVideoPreview`.
 */
export function useTransitionProxyPlayback(proxyRef, transition) {
  const proxyUrl = transition?.proxyUrl ?? null
  const elapsedSeconds = transition?.elapsedSeconds ?? 0
  const isPlaying = transition?.isPlaying ?? false

  useEffect(() => {
    const video = proxyRef.current
    if (!video || proxyUrl === null) return
    if (!isPlaying) {
      if (!video.paused) video.pause()
      syncVideoCurrentTime(video, elapsedSeconds)
      return
    }
    if (Math.abs(video.currentTime - elapsedSeconds) > PROXY_DRIFT_SECONDS) syncVideoCurrentTime(video, elapsedSeconds)
    if (video.paused) {
      video.play()?.catch((error) => {
        if (error?.name !== 'AbortError') console.error('[useTransitionProxyPlayback] Failed to play the transition preview', error)
      })
    }
  }, [elapsedSeconds, isPlaying, proxyRef, proxyUrl])
}
