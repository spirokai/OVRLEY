/**
 * Manages metadata, error, and seek-latency warnings for the preview video.
 */

import { useEffect, useRef, useState } from 'react'
import { METADATA_SOFT_WARNING_MS, METADATA_STRONG_WARNING_MS, SLOW_SEEK_WARNING_COUNT, SLOW_SEEK_WARNING_MS } from '../data/videoPreviewConstants'
import { describeMediaError } from '../utils/videoPreviewPlayback'

/**
 * Tracks user-visible warning state for the active preview video.
 *
 * @param {object} options - Warning inputs.
 * @param {function} options.setImportedVideoPreviewError - Store action that mirrors native preview errors.
 * @param {React.RefObject<HTMLVideoElement>} options.videoRef - Preview video ref.
 * @param {string} options.videoSrc - Active preview video source URL.
 * @returns {{ metadataStatusMessage: string, nativeVideoError: string, seekWarning: string }} Warning state.
 */
export function useVideoPreviewWarnings({ setImportedVideoPreviewError, videoRef, videoSrc }) {
  const [metadataStatusMessage, setMetadataStatusMessage] = useState('')
  const [nativeVideoError, setNativeVideoError] = useState('')
  const [seekWarning, setSeekWarning] = useState('')

  const metadataSoftTimerRef = useRef(null)
  const metadataStrongTimerRef = useRef(null)
  const seekStartedAtMsRef = useRef(null)
  const slowSeekCountRef = useRef(0)

  const clearMetadataTimers = () => {
    if (metadataSoftTimerRef.current !== null) {
      window.clearTimeout(metadataSoftTimerRef.current)
      metadataSoftTimerRef.current = null
    }

    if (metadataStrongTimerRef.current !== null) {
      window.clearTimeout(metadataStrongTimerRef.current)
      metadataStrongTimerRef.current = null
    }
  }

  useEffect(() => {
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

  useEffect(() => {
    const video = videoRef.current
    if (!video || !videoSrc) {
      return undefined
    }

    const handleLoadedMetadata = () => {
      clearMetadataTimers()
      setMetadataStatusMessage('')
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
  }, [setImportedVideoPreviewError, videoRef, videoSrc])

  return {
    metadataStatusMessage,
    nativeVideoError,
    seekWarning,
  }
}
