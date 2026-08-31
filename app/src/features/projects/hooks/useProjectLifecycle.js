import { useCallback, useEffect, useRef, useState } from 'react'
import * as backend from '@/api/backend'
import { openSinglePath, saveSinglePath } from '@/lib/file-dialog'
import { getPreference } from '@/lib/preferences-store'
import { pathInDirectory } from '@/lib/utils'
import { useUnsavedChangesConfirm } from '@/features/app-shell'
import useStore from '@/store/useStore'
import { createNewProject, loadProject, saveProject } from '../projectOperations'
import { LAST_PROJECT_DIRECTORY_KEY } from '../utils/projectSnapshot'
import useProjectDocumentState from './useProjectDocumentState'
import useProjectSourceRecovery from './useProjectSourceRecovery'

const PROJECT_FILTER = [{ name: 'OVRLEY Project', extensions: ['oly'] }]

/**
 * Coordinates project commands with project UI state.
 * @param {object} options Project media-owner operations.
 * @param {function} options.prepareActivityPath Parses one activity source without committing it.
 * @param {function} options.clearImportedVideo Clears the active video source.
 * @param {function} options.prepareVideoPath Probes one video source without committing it.
 * @param {function} [options.onSetBackgroundMode] Shell setter for the editor background mode.
 * @param {boolean} [options.startupReady] Whether application startup completed successfully.
 * @param {function} [options.onCreateBlankTemplate] Creates the blank template selected from the startup dialog.
 * @returns {object} Project lifecycle state and command handlers.
 */
export default function useProjectLifecycle({
  prepareActivityPath,
  clearImportedVideo,
  prepareVideoPath,
  onSetBackgroundMode,
  startupReady = false,
  onCreateBlankTemplate,
}) {
  const { conflictingOperation, loadedProjectPath, markNew, markSaved, projectName, status } = useProjectDocumentState()
  const { dialog: missingSourceDialog, resolveProjectSources } = useProjectSourceRecovery()
  const {
    answerConfirm: answerUnsavedChangesConfirm,
    isOpen: isNewProjectConfirmOpen,
    requestConfirm: requestUnsavedChangesConfirm,
  } = useUnsavedChangesConfirm()
  const [activeOperation, setActiveOperation] = useState(null)
  const [startupDialogOpen, setStartupDialogOpen] = useState(false)
  const [startupProjects, setStartupProjects] = useState([])
  const [startupOpeningPath, setStartupOpeningPath] = useState(null)
  const operationLock = useRef(false)
  const startupLoaded = useRef(false)
  const busy = activeOperation !== null || conflictingOperation

  useEffect(() => {
    if (!startupReady || startupLoaded.current) return
    startupLoaded.current = true

    const loadStartupProjects = async () => {
      try {
        const rememberedDirectory = await getPreference(LAST_PROJECT_DIRECTORY_KEY)
        if (rememberedDirectory !== null && rememberedDirectory !== undefined && typeof rememberedDirectory !== 'string') {
          throw new Error('Saved project directory must be a path string')
        }
        if (typeof rememberedDirectory === 'string' && !rememberedDirectory.trim()) {
          throw new Error('Saved project directory must not be empty')
        }
        const directory = rememberedDirectory ?? (await backend.getDefaultProjectDirectory())
        setStartupProjects(await backend.listProjectFiles(directory))
      } catch (error) {
        console.error('Failed to list startup projects:', error)
        const message = error instanceof Error ? error.message : String(error)
        useStore.getState().setErrorMessage(`Failed to list projects: ${message}`)
      } finally {
        setStartupDialogOpen(true)
      }
    }

    loadStartupProjects()
  }, [startupReady])

  const runOperation = useCallback(
    async (operationName, operation) => {
      if (operationLock.current || conflictingOperation) return false
      operationLock.current = true
      setActiveOperation(operationName)
      try {
        return await operation()
      } catch (error) {
        console.error(`Failed to ${operationName}:`, error)
        useStore.getState().setErrorMessage(`Failed to ${operationName}: ${error.message}`)
        return false
      } finally {
        operationLock.current = false
        setActiveOperation(null)
      }
    },
    [conflictingOperation],
  )

  const loadProjectPath = useCallback(
    async (path) => {
      const project = await loadProject({ path, resolveProjectSources, prepareActivityPath, prepareVideoPath, onSetBackgroundMode })
      if (!project) return false
      markSaved(path, project)
      return true
    },
    [markSaved, onSetBackgroundMode, prepareActivityPath, prepareVideoPath, resolveProjectSources],
  )

  const handleOpenProject = useCallback(
    () =>
      runOperation('open project', async () => {
        const defaultPath = await backend.getDefaultProjectDirectory()
        const path = await openSinglePath(PROJECT_FILTER, { defaultPath, lastDirectoryKey: LAST_PROJECT_DIRECTORY_KEY })
        return path ? loadProjectPath(path) : false
      }),
    [loadProjectPath, runOperation],
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

  const handleOpenProjectPath = useCallback((path) => runOperation('open project', () => loadProjectPath(path)), [loadProjectPath, runOperation])

  const handleStartupNewProject = useCallback(async () => {
    await onCreateBlankTemplate()
    setStartupDialogOpen(false)
  }, [onCreateBlankTemplate])

  const handleStartupOpenProject = useCallback(
    async (path) => {
      setStartupOpeningPath(path)
      setStartupDialogOpen(false)
      await handleOpenProjectPath(path)
      setStartupOpeningPath(null)
    },
    [handleOpenProjectPath],
  )

  // Create new project — asks to save unsaved changes before discarding them
  const handleNewProject = useCallback(async () => {
    if (status !== 'Saved') {
      const action = await requestUnsavedChangesConfirm()
      if (action === 'cancel') return false
      if (action === 'save') {
        const saved = await save(false)
        if (!saved) return false
      }
    }

    return runOperation('create project', async () => {
      await createNewProject({ clearImportedVideo })
      markNew()
      return true
    })
  }, [clearImportedVideo, markNew, requestUnsavedChangesConfirm, runOperation, save, status])

  const newProjectConfirmDialog = {
    open: isNewProjectConfirmOpen,
    title: 'Create New Project',
    description: 'Your project has unsaved changes. Save them or discard them.',
    discardLabel: 'New Project',
    onCancel: () => answerUnsavedChangesConfirm('cancel'),
    onSave: () => answerUnsavedChangesConfirm('save'),
    onDiscard: () => answerUnsavedChangesConfirm('discard'),
  }

  return {
    busy,
    handleNewProject,
    handleOpenProject,
    handleSaveProject,
    handleSaveProjectAs,
    loadedProjectPath,
    loadingProject: activeOperation === 'open project',
    missingSourceDialog,
    newProjectConfirmDialog,
    projectName,
    startupProjectDialog: {
      open: startupDialogOpen,
      openingPath: startupOpeningPath,
      projects: startupProjects,
      onDismiss: () => setStartupDialogOpen(false),
      onNewProject: handleStartupNewProject,
      onOpenProject: handleStartupOpenProject,
    },
    status,
  }
}
