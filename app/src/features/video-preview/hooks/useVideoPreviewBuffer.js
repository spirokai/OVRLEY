import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { primeVideoFirstFrame } from '../utils/videoPreviewPlayback'

/**
 * Keeps the outgoing frame visible until the incoming chapter has decoded a frame.
 * The next chapter is preloaded in its own element and promoted without remounting.
 * @param {object} options Active preview ref/source and optional next chapter.
 * @returns {object} Presentational video layers and their element registration.
 */
export function useVideoPreviewBuffer({ videoRef, videoSrc, importId, nextVideoSource }) {
  const elements = useRef(new Map())
  const [presentedSource, setPresentedSource] = useState(null)
  const registerElement = useCallback((key, element) => {
    if (element) elements.current.set(key, element)
    else elements.current.delete(key)
  }, [])
  const activeSource = useMemo(() => {
    if (!videoSrc) return null
    const key = importId ?? videoSrc
    return { key, src: videoSrc }
  }, [importId, videoSrc])

  // Ref ownership must change before the playback/warning hooks bind their effects.
  useLayoutEffect(() => {
    videoRef.current = activeSource ? elements.current.get(activeSource.key) : null
    for (const [key, video] of elements.current) {
      if (key !== activeSource?.key && !video.paused) video.pause()
    }
  })

  useEffect(() => {
    if (!activeSource) {
      setPresentedSource(null)
      return undefined
    }
    const video = videoRef.current
    let disposed = false
    let revealed = false
    let frameCallback = null
    let animationFrame = null

    const reveal = () => {
      if (disposed || revealed || video.seeking || video.readyState < 2) return
      revealed = true
      setPresentedSource(activeSource)
    }
    const waitForDecodedFrame = () => {
      if (disposed || revealed || video.seeking || video.readyState < 2) return
      if (frameCallback === null && video.requestVideoFrameCallback) {
        frameCallback = video.requestVideoFrameCallback(reveal)
      }
      // Paused/preloaded video elements may not receive compositor callbacks.
      // Two paint opportunities after decoded data also cover that case.
      if (animationFrame === null) {
        animationFrame = requestAnimationFrame(() => {
          animationFrame = requestAnimationFrame(() => {
            animationFrame = null
            reveal()
          })
        })
      }
    }
    for (const event of ['loadeddata', 'seeked', 'playing']) video.addEventListener(event, waitForDecodedFrame)
    waitForDecodedFrame()
    return () => {
      disposed = true
      for (const event of ['loadeddata', 'seeked', 'playing']) video.removeEventListener(event, waitForDecodedFrame)
      if (frameCallback !== null) video.cancelVideoFrameCallback(frameCallback)
      if (animationFrame !== null) cancelAnimationFrame(animationFrame)
    }
  }, [activeSource, videoRef])

  if (!activeSource) return { layers: [], registerElement }
  const sources = new Map()
  for (const source of [presentedSource, activeSource, nextVideoSource]) {
    if (source) sources.set(source.key, source)
  }
  const visibleKey = presentedSource?.key ?? activeSource.key
  const layers = [...sources.values()].map((source) => ({
    ...source,
    isActive: source.key === activeSource.key,
    isVisible: source.key === visibleKey,
    onLoadedMetadata: (event) => {
      if (source.key !== activeSource.key) primeVideoFirstFrame(event.currentTarget)
    },
  }))
  return { layers, registerElement }
}

/** @param {string} key Preview identity. @param {Function} registerElement Buffer registration. @returns {Function} Stable element ref. */
export function useVideoPreviewLayerRef(key, registerElement) {
  return useCallback(
    (element) => {
      registerElement(key, element)
      if (!element) return undefined
      return () => {
        registerElement(key, null)
      }
    },
    [key, registerElement],
  )
}
