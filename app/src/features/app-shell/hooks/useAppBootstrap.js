/**
 * App bootstrap — one-time initialization effects on mount.
 * Hydrates platform OS info and fetches templates and available codecs.
 */

import { useEffect } from 'react'
import * as backend from '@/api/backend'
import { useBootstrapStore } from '@/hooks/useAppStoreSelectors'

/**
 * Container hook for app-level bootstrap effects.
 * Runs on mount to detect the platform OS and pre-load templates/codecs.
 *
 * @returns {void}
 */
export default function useAppBootstrap() {
  // Store selectors — bootstrap action dispatchers from the global store
  const { fetchAvailableCodecs, fetchTemplates, setPlatformOs } = useBootstrapStore()

  // Platform OS detection — reads OS info from the Tauri backend on mount
  useEffect(() => {
    let cancelled = false

    const hydratePlatformOs = async () => {
      try {
        const platformInfo = await backend.getPlatformInfo()
        if (!cancelled) {
          setPlatformOs(platformInfo?.os || 'unknown')
        }
      } catch (error) {
        console.error('Failed to read platform info:', error)
      }
    }

    hydratePlatformOs()
    return () => {
      cancelled = true
    }
  }, [setPlatformOs])

  // Template and codec data fetching — pre-load templates and available codecs on mount
  useEffect(() => {
    fetchTemplates()
    fetchAvailableCodecs()
  }, [fetchAvailableCodecs, fetchTemplates])
}
