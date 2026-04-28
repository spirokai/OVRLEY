import { useEffect, useState } from 'react'
import * as backend from '@/api/backend'

let cachedFonts = null
let pendingFontsPromise = null

async function loadAvailableFonts() {
  if (cachedFonts) {
    return cachedFonts
  }

  if (!pendingFontsPromise) {
    pendingFontsPromise = backend
      .listAvailableFonts()
      .then((fonts) => {
        cachedFonts = fonts
        return fonts
      })
      .catch((error) => {
        pendingFontsPromise = null
        throw error
      })
  }

  return pendingFontsPromise
}

export default function useAvailableFonts() {
  const [systemFonts, setSystemFonts] = useState(cachedFonts || [])

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
