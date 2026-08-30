import { useCallback, useState } from 'react'
import * as backend from '@/api/backend'
import { openSinglePath, saveSinglePath } from '@/lib/file-dialog'
import useStore from '@/store/useStore'
import { createNewProject, loadProject, saveProject } from '../projectOperations'
import { pathInDirectory } from '../utils/projectPaths'
import { LAST_PROJECT_DIRECTORY_KEY } from '../utils/projectSnapshot'
import useProjectDocumentState from './useProjectDocumentState'
import useProjectSourceRecovery from './useProjectSourceRecovery'

const PROJECT_FILTER = [{ name: 'OVRLEY Project', extensions: ['oly'] }]

/**
 * Coordinates project commands with project UI state.
 * @param {object} options Project media-owner operations.
 * @param {function} options.loadActivityPath Loads one activity source path.
 * @param {function} options.clearImportedVideo Clears the active video source.
 * @param {function} options.loadVideoPath Loads one video source path.
 * @returns {object} Project lifecycle state and command handlers.
 */
export default function useProjectLifecycle({ loadActivityPath, clearImportedVideo, loadVideoPath }) {
  const { conflictingOperation, loadedProjectPath, markNew, markSaved, projectName, status } = useProjectDocumentState()
  const { dialog: missingSourceDialog, resolveProjectSources } = useProjectSourceRecovery()
  const [operationBusy, setOperationBusy] = useState(false)
  const busy = operationBusy || conflictingOperation

  const runOperation = useCallback(
    async (operationName, operation) => {
      if (busy) return false
      setOperationBusy(true)
      try {
        return await operation()
      } catch (error) {
        console.error(`Failed to ${operationName}:`, error)
        useStore.getState().setErrorMessage(`Failed to ${operationName}: ${error.message}`)
        return false
      } finally {
        setOperationBusy(false)
      }
    },
    [busy],
  )

  const handleOpenProject = useCallback(async () => {
    if (busy) return
    try {
      const defaultPath = await backend.getDefaultProjectDirectory()
      const path = await openSinglePath(PROJECT_FILTER, { defaultPath, lastDirectoryKey: LAST_PROJECT_DIRECTORY_KEY })
      if (!path) return false
      return runOperation('open project', async () => {
        const project = await loadProject({ path, resolveProjectSources, loadActivityPath, loadVideoPath, clearImportedVideo })
        if (!project) return false
        markSaved(path, project)
        return true
      })
    } catch (error) {
      console.error('Failed to open project picker:', error)
      useStore.getState().setErrorMessage(`Failed to open project picker: ${error.message}`)
      return false
    }
  }, [busy, clearImportedVideo, loadActivityPath, loadVideoPath, markSaved, resolveProjectSources, runOperation])

  const handleNewProject = useCallback(
    () =>
      runOperation('create project', async () => {
        await createNewProject({ clearImportedVideo })
        markNew()
        return true
      }),
    [clearImportedVideo, markNew, runOperation],
  )

  const save = useCallback(
    (saveAs) =>
      runOperation('save project', async () => {
        let path = loadedProjectPath
        if (saveAs || !path) {
          const defaultDirectory = await backend.getDefaultProjectDirectory()
          path = await saveSinglePath(path || pathInDirectory(defaultDirectory, 'project.oly'), 'oly', 'OVRLEY Project', {
            lastDirectoryKey: LAST_PROJECT_DIRECTORY_KEY,
          })
        }
        if (!path) return false
        const project = await saveProject(path)
        markSaved(path, project)
        return true
      }),
    [loadedProjectPath, markSaved, runOperation],
  )

  const handleSaveProject = useCallback(() => save(false), [save])
  const handleSaveProjectAs = useCallback(() => save(true), [save])

  return {
    busy,
    handleNewProject,
    handleOpenProject,
    handleSaveProject,
    handleSaveProjectAs,
    loadedProjectPath,
    missingSourceDialog,
    projectName,
    status,
  }
}
