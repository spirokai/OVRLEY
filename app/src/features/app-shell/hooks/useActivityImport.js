/**
 * Activity import - native activity file selection and import.
 */

import { useCallback } from 'react'
import { useActivityStore } from '@/hooks/useAppStoreSelectors'
import { importActivityPath, parseActivityPath } from '@/lib/import-activity'
import { openSinglePath } from '@/lib/file-dialog'
import { runWithoutEditorHistory } from '@/features/undo-redo/undoHistory'
import useStore from '@/store/useStore'

export default function useActivityImport() {
  const {
    activityFilename,
    activitySource,
    activitySummary,
    clearActivityFile,
    disabledLaps,
    parsedActivitySource,
    setErrorMessage,
    setLapEnabled,
    setProcessing,
  } = useActivityStore()

  const loadActivitySelection = useCallback(
    async (path) => {
      setProcessing(true)
      try {
        await runWithoutEditorHistory(useStore, () => importActivityPath(path, useStore.getState()))
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
      const selectedPath = await openSinglePath(
        [{ name: 'Activity', extensions: ['gpx', 'fit', 'tcx', 'srt', 'igc', 'csv', 'vbo', 'uddf', 'ssrf', 'xml'] }],
        {
          lastDirectoryKey: 'last-activity-import-dir',
        },
      )
      if (typeof selectedPath === 'string') await importSelection(selectedPath)
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
    activityPath: activitySource?.path ?? null,
    activitySummary,
    deleteActivity: parsedActivitySource === 'activity-file' ? clearActivityFile : null,
    disabledLaps,
    handleActivityFileOpen,
    handleActivityFilesDrop,
    loadActivityPath: loadActivitySelection,
    prepareActivityPath: parseActivityPath,
    setLapEnabled,
  }
}
