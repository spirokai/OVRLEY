/**
 * Returns a version token that changes after the requested font becomes ready.
 */

import { useEffect, useState } from 'react'

export function useFontMetricsVersion(fontFamily, fontSize) {
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

    return () => {
      cancelled = true
    }
  }, [fontFamily, fontSize])

  return version
}
