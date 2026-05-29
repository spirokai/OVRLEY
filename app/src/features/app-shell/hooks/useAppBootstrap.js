/**
 * App bootstrap - one-time initialization effects on mount.
 * Hydrates platform OS info and fetches templates and available codecs.
 *
 * Template fetching uses the dedicated useTemplateFetching orchestration hook
 * instead of calling a store action that performs network I/O directly.
 */

import { useEffect } from 'react'
import * as backend from '@/api/backend'
import { useBootstrapStore } from '@/hooks/useAppStoreSelectors'
import useTemplateFetching from '@/features/template-manager/hooks/useTemplateFetching'

/**
 * Container hook for app-level bootstrap effects.
 * Runs on mount to detect the platform OS and preload templates/codecs.
 *
 * @returns {void}
 */
export default function useAppBootstrap() {
  // Store selectors - bootstrap action dispatchers from the global store.
  const { fetchAvailableCodecs, setPlatformOs } = useBootstrapStore()
  const { fetchTemplates } = useTemplateFetching()

  // Platform OS detection - reads OS info from the Tauri backend on mount.
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

  // Template and codec data fetching - preload templates and available codecs.
  useEffect(() => {
    fetchTemplates()
    fetchAvailableCodecs()
  }, [fetchAvailableCodecs, fetchTemplates])
}
