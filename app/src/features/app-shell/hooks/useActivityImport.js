/**
 * Activity import - GPX/FIT file selection and import.
 */

import { useCallback } from 'react'
import { hasTauriRuntime } from '@/api/backend'
import { useActivityStore } from '@/hooks/useAppStoreSelectors'
import importActivityFile from '@/lib/activity/import-activity'
import { fileFromSelectedPath, openSinglePath, selectBrowserFile } from '@/lib/file-dialog'
import useStore from '@/store/useStore'

export default function useActivityImport() {
  const { activityFilename, setErrorMessage, setProcessing } = useActivityStore()

  const handleGpxFileOpen = useCallback(async () => {
    try {
      let selected = null

      if (hasTauriRuntime()) {
        const selectedPath = await openSinglePath([{ name: 'GPX, FIT or SRT', extensions: ['gpx', 'fit', 'srt'] }])

        if (typeof selectedPath === 'string') {
          selected = await fileFromSelectedPath(selectedPath, 'activity')
        }
      } else {
        selected = await selectBrowserFile('.gpx,.fit,.srt')
      }

      if (!selected) return

      setProcessing(true)
      await importActivityFile(selected, useStore.getState())
    } catch (error) {
      console.error('GPX selection failed:', error)
      setErrorMessage(`GPX Selection failed: ${error.message}`)
    } finally {
      setProcessing(false)
    }
  }, [setErrorMessage, setProcessing])

  return {
    activityFilename,
    handleGpxFileOpen,
  }
}
