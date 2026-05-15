/**
 * Activity import — GPX/FIT file selection via browser file dialog.
 * Manages the file-open flow and delegates parsing to gpxUtils.
 */

import { useCallback } from 'react'
import { useActivityStore } from '@/hooks/useAppStoreSelectors'

/**
 * Opens a browser file-picker dialog for .gpx / .fit files.
 * Returns the selected File object, or null on cancel.
 *
 * @returns {Promise<File|null>} The selected file or null.
 */
const selectBrowserGpxFile = () =>
  new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.gpx,.fit'
    input.onchange = () => resolve(input.files?.[0] ?? null)
    input.oncancel = () => resolve(null)
    input.click()
  })

/**
 * Container hook for activity file import.
 * Provides the current GPX filename and an open-handler that
 * triggers the browser file dialog and saves the selected file.
 *
 * @returns {{
 *   gpxFilename: string|null,
 *   handleGpxFileOpen: Function,
 * }}
 */
export default function useActivityImport() {
  // Store selectors — activity import state from the global store
  const { gpxFilename, setErrorMessage, setGeneratingImage } = useActivityStore()

  // Activity file handler — opens file dialog, imports GPX/FIT, handles errors
  const handleGpxFileOpen = useCallback(async () => {
    try {
      const { default: saveFileFromPath } = await import('@/lib/activity/import-activity')
      const selected = await selectBrowserGpxFile()

      if (!selected) return

      setGeneratingImage(true)
      await saveFileFromPath(selected)
    } catch (error) {
      console.error('GPX selection failed:', error)
      setErrorMessage(`GPX Selection failed: ${error.message}`)
    } finally {
      setGeneratingImage(false)
    }
  }, [setErrorMessage, setGeneratingImage])

  return {
    gpxFilename,
    handleGpxFileOpen,
  }
}
