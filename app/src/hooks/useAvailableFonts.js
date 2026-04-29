/**
 * Implements the use Available Fonts hook and related behavior for the app.
 */

import { useEffect, useState } from 'react'
import * as backend from '@/api/backend'

let cachedFonts = null
let pendingFontsPromise = null

/**
 * Handles load available fonts.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
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

/**
 * Provides available fonts state and actions.
 * @returns {*} Result produced by the helper.
 */
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
