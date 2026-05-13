/**
 * Implements the use App Bootstrap hook and related behavior for the app.
 */

import { useEffect } from 'react'
import * as backend from '@/api/backend'
import { useBootstrapStore } from '@/hooks/useAppStoreSelectors'

/**
 * Provides app bootstrap state and actions.
 * @returns {*} Result produced by the helper.
 */
export default function useAppBootstrap() {
  const { fetchAvailableCodecs, fetchTemplates, setPlatformOs } = useBootstrapStore()

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

  useEffect(() => {
    fetchTemplates()
    fetchAvailableCodecs()
  }, [fetchAvailableCodecs, fetchTemplates])
}
