/**
 * Implements the use Activity Import hook and related behavior for the app.
 */

import { useCallback } from 'react'
import { useActivityStore } from '@/hooks/useAppStoreSelectors'

/**
 * Handles select browser gpx file.
 * @returns {*} Result produced by the helper.
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
 * Provides activity import state and actions.
 * @returns {object} Result produced by the helper.
 */
export default function useActivityImport() {
  const { gpxFilename, setErrorMessage, setGeneratingImage } =
    useActivityStore()

  const handleGpxFileOpen = useCallback(async () => {
    try {
      const { default: saveFileFromPath } = await import('../api/gpxUtils')
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
