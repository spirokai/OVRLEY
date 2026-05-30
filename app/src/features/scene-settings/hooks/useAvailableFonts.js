/**
 * Provides available fonts state from system and bundled fonts.
 */

import { useEffect, useState } from 'react'
import * as backend from '@/api/backend'
import { createCachedPromise } from '@/lib/cached-promise'

const loadAvailableFonts = createCachedPromise(() => backend.listAvailableFonts())

const initialFonts = (() => {
  try {
    const cached = loadAvailableFonts()
    return cached instanceof Promise ? [] : cached
  } catch {
    return []
  }
})()

export default function useAvailableFonts() {
  const [systemFonts, setSystemFonts] = useState(initialFonts)

  useEffect(() => {
    let cancelled = false

    loadAvailableFonts()
      .then((fonts) => {
        if (!cancelled) {
          setSystemFonts(fonts)
        }
      })
      .catch((error) => {
        console.warn('Failed to load available fonts:', error)
      })

    return () => {
      cancelled = true
    }
  }, [])

  return systemFonts
}
