/**
 * App bootstrap - one-time initialization effects on mount.
 * Initializes the widget renderer, hydrates platform OS info, and fetches templates/codecs.
 *
 * Template fetching uses the dedicated useTemplateFetching orchestration hook
 * instead of calling a store action that performs network I/O directly.
 */

import { useEffect, useState } from 'react'
import * as backend from '@/api/backend'
import { useBootstrapStore } from '@/hooks/useAppStoreSelectors'
import useTemplateFetching from '@/features/template-manager/hooks/useTemplateFetching'
import { loadWidgetModule } from '@/features/wasm-widgets/utils/previewSession'

/**
 * Container hook for app-level bootstrap effects.
 * Runs on mount to initialize the widget renderer and preload platform/editor resources.
 *
 * @returns {{ready: boolean, error: Error|null}} Widget renderer readiness and startup failure.
 */
export default function useAppBootstrap() {
  // Store selectors - bootstrap action dispatchers from the global store.
  const { fetchAvailableCodecs, setPlatformOs } = useBootstrapStore()
  const { fetchTemplates } = useTemplateFetching()
  const [bootstrap, setBootstrap] = useState({ ready: false, error: null })

  useEffect(() => {
    let cancelled = false
    loadWidgetModule().then(
      () => {
        if (!cancelled) setBootstrap({ ready: true, error: null })
      },
      (error) => {
        if (!cancelled) setBootstrap({ ready: false, error })
      },
    )
    return () => {
      cancelled = true
    }
  }, [])

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

  return bootstrap
}
