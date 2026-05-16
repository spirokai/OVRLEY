/**
 * Returns a version token that changes after the requested font becomes ready.
 *
 * Used to trigger re-renders of preview components once font metrics are available,
 * ensuring accurate text measurement after font loading.
 *
 * @param {string} fontFamily - Font family to await.
 * @param {number} fontSize - Font size in pixels to load.
 * @returns {number} Version counter — increments each time the font finishes loading.
 */

import { useEffect, useState } from 'react'

export function useFontMetricsVersion(fontFamily, fontSize) {
  // State — version token incremented when font metrics are refreshed
  const [version, setVersion] = useState(0)

  useEffect(() => {
    if (typeof document === 'undefined' || !document.fonts || typeof document.fonts.load !== 'function') {
      return undefined
    }

    let cancelled = false

    const refreshMetrics = async () => {
      try {
        await Promise.allSettled([document.fonts.load(`${fontSize}px ${fontFamily}`, '0123456789WBMPRK/H'), document.fonts.ready])
      } finally {
        if (!cancelled) {
          setVersion((current) => current + 1)
        }
      }
    }

    refreshMetrics()

    // Cleanup — cancels pending font load on unmount or re-render
    return () => {
      cancelled = true
    }
  }, [fontFamily, fontSize])

  return version
}
