/**
 * Activity import - GPX/FIT/SRT/IGC/CSV/VBO file selection and import.
 */

import { useCallback } from 'react'
import { hasTauriRuntime } from '@/api/backend'
import { useActivityStore } from '@/hooks/useAppStoreSelectors'
import importActivityFile, { importActivityPath, parseActivityPath } from '@/lib/activity/import-activity'
import { openSinglePath, selectBrowserFile } from '@/lib/file-dialog'
import { runWithoutEditorHistory } from '@/features/undo-redo/undoHistory'
import useStore from '@/store/useStore'

export default function useActivityImport() {
  const { activityFilename, activitySummary, clearActivityFile, parsedActivitySource, setErrorMessage, setProcessing } = useActivityStore()

  const loadActivitySelection = useCallback(
    async (selection) => {
      let runImport
      if (typeof selection === 'string') {
        runImport = () => importActivityPath(selection, useStore.getState())
      } else if (selection instanceof File) {
        runImport = () => importActivityFile(selection, useStore.getState())
      } else {
        throw new Error('Activity import requires one file.')
      }
      setProcessing(true)
      try {
        await runWithoutEditorHistory(useStore, runImport)
      } finally {
        setProcessing(false)
      }
    },
    [setProcessing],
  )

  const importSelection = useCallback(
    async (selection) => {
      try {
        await loadActivitySelection(selection)
      } catch (error) {
        console.error('Activity selection failed:', error)
        setErrorMessage(`Activity selection failed: ${error.message}`)
      }
    },
    [loadActivitySelection, setErrorMessage],
  )

  const handleActivityFileOpen = useCallback(async () => {
    try {
      let selection = null

      if (hasTauriRuntime()) {
        const selectedPath = await openSinglePath([{ name: 'Activity', extensions: ['gpx', 'fit', 'srt', 'igc', 'csv', 'vbo'] }], {
          lastDirectoryKey: 'last-activity-import-dir',
        })
        if (typeof selectedPath === 'string') selection = selectedPath
      } else {
        const selectedFile = await selectBrowserFile('.gpx,.fit,.srt,.igc')
        if (selectedFile) selection = selectedFile
      }

      if (selection) await importSelection(selection)
    } catch (error) {
      console.error('Activity selection failed:', error)
      setErrorMessage(`Activity selection failed: ${error.message}`)
    }
  }, [importSelection, setErrorMessage])

  const handleActivityFilesDrop = useCallback(
    async (selections) => {
      if (selections.length !== 1) {
        setErrorMessage('Activity selection failed: Drop exactly one activity file.')
        return
      }

      await importSelection(selections[0])
    },
    [importSelection, setErrorMessage],
  )

  return {
    activityFilename,
    activitySummary,
    deleteActivity: parsedActivitySource === 'activity-file' ? clearActivityFile : null,
    handleActivityFileOpen,
    handleActivityFilesDrop,
    loadActivityPath: loadActivitySelection,
    prepareActivityPath: parseActivityPath,
  }
}
