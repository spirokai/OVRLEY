/**
 * Provides available fonts state from system and bundled fonts.
 */

import { useEffect, useState } from 'react'
import * as backend from '@/api/backend'

const loadAvailableFonts = backend.listAvailableFonts
const EMPTY_AVAILABLE_FONTS = {
  recommendedFonts: [],
  systemFonts: [],
}

const initialFonts = (() => {
  try {
    const cached = loadAvailableFonts()
    return cached instanceof Promise ? EMPTY_AVAILABLE_FONTS : cached
  } catch {
    return EMPTY_AVAILABLE_FONTS
  }
})()

export default function useAvailableFonts() {
  const [availableFonts, setAvailableFonts] = useState(initialFonts)

  useEffect(() => {
    let cancelled = false

    loadAvailableFonts()
      .then((fonts) => {
        if (!cancelled) {
          setAvailableFonts(fonts)
        }
      })
      .catch((error) => {
        console.warn('Failed to load available fonts:', error)
      })

    return () => {
      cancelled = true
    }
  }, [])

  return availableFonts
}
