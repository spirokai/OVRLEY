import { useEffect, useRef } from 'react'
import { useLayoutStore } from '@/hooks/useAppStoreSelectors'
import { getPreference, setPreference } from '@/lib/preferences-store'
import { WIDGETS_TOOL } from '@/store/slices/createLayoutSlice'

const PREFERENCE_KEY = 'leftDrawer'
const DEFAULT_PREFERENCE = Object.freeze({
  pinned: false,
  activeTool: WIDGETS_TOOL,
})

function normalizePreference(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || typeof value.pinned !== 'boolean' || value.activeTool !== WIDGETS_TOOL) {
    return { ...DEFAULT_PREFERENCE }
  }

  return {
    pinned: value.pinned,
    activeTool: value.activeTool,
  }
}

function preferencesMatch(left, right) {
  return left?.pinned === right.pinned && left?.activeTool === right.activeTool
}

/**
 * Hydrates and persists the optional durable shared-drawer preference.
 *
 * @returns {boolean} Whether the shell layout preference has initialized.
 */
export function useDrawerPreference() {
  const { activeLeftDrawerTool, initializeLeftDrawer, leftDrawerInitialized, leftDrawerPinned } = useLayoutStore()
  const lastObservedPreferenceRef = useRef(null)
  const writeQueueRef = useRef(Promise.resolve())

  useEffect(() => {
    let mounted = true

    async function hydratePreference() {
      let storedPreference
      try {
        storedPreference = await getPreference(PREFERENCE_KEY)
      } catch {
        storedPreference = undefined
      }

      if (!mounted) return

      const preference = normalizePreference(storedPreference)
      lastObservedPreferenceRef.current = preference
      initializeLeftDrawer(preference)
    }

    hydratePreference()
    return () => {
      mounted = false
    }
  }, [initializeLeftDrawer])

  useEffect(() => {
    if (!leftDrawerInitialized) return

    const preference = {
      pinned: leftDrawerPinned,
      activeTool: activeLeftDrawerTool,
    }
    if (preferencesMatch(lastObservedPreferenceRef.current, preference)) return

    lastObservedPreferenceRef.current = preference
    writeQueueRef.current = writeQueueRef.current.then(() => setPreference(PREFERENCE_KEY, preference)).catch(() => undefined)
  }, [activeLeftDrawerTool, leftDrawerInitialized, leftDrawerPinned])

  return leftDrawerInitialized
}
