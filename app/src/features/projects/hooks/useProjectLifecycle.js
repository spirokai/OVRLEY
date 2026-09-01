import { useCallback, useEffect, useRef, useState } from 'react'
import * as backend from '@/api/backend'
import { openSinglePath, saveSinglePath } from '@/lib/file-dialog'
import { getOptionalPathPreference } from '@/lib/preferences-store'
import { pathInDirectory } from '@/lib/utils'
import { useUnsavedChangesConfirm } from '@/features/app-shell'
import useStore from '@/store/useStore'
import { createNewProject, loadProject, saveProject } from '../projectOperations'
import { LAST_PROJECT_DIRECTORY_KEY } from '../utils/projectSnapshot'
import useProjectDocumentState from './useProjectDocumentState'
import useProjectSourceRecovery from './useProjectSourceRecovery'
import i18next from 'i18next'

const PROJECT_FILTER = [{ name: 'OVRLEY Project', extensions: ['oly'] }]

/**
 * Coordinates project commands with project UI state.
 * @param {object} options Project media-owner operations.
 * @param {function} options.prepareActivityPath Parses one activity source without committing it.
 * @param {function} options.clearImportedVideo Clears the active video source.
 * @param {function} options.prepareVideoPath Probes one video source without committing it.
 * @param {function} [options.onSetBackgroundMode] Shell setter for the editor background mode.
 * @param {boolean} [options.startupReady] Whether application startup completed successfully.
 * @returns {object} Project lifecycle state and command handlers.
 */
export default function useProjectLifecycle({
  prepareActivityPath,
  clearImportedVideo,
  prepareVideoPath,
  onSetBackgroundMode,
  startupReady = false,
}) {
  const { conflictingOperation, loadedProjectPath, markNew, markSaved, projectName, status } = useProjectDocumentState()
  const { dialog: missingSourceDialog, resolveProjectSources } = useProjectSourceRecovery()
  const { answerConfirm: answerProjectConfirm, isOpen: isProjectConfirmOpen, requestConfirm: requestProjectConfirm } = useUnsavedChangesConfirm()
  const [activeOperation, setActiveOperation] = useState(null)
  const [confirmationIntent, setConfirmationIntent] = useState('new')
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
        const rememberedDirectory = await getOptionalPathPreference(LAST_PROJECT_DIRECTORY_KEY)
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
      runOperation(i18next.t('projects.openProject', 'open project'), async () => {
        const defaultPath = await backend.getDefaultProjectDirectory()
        const path = await openSinglePath(PROJECT_FILTER, { defaultPath, lastDirectoryKey: LAST_PROJECT_DIRECTORY_KEY })
        return path ? loadProjectPath(path) : false
      }),
    [loadProjectPath, runOperation],
  )

  const save = useCallback(
    (saveAs) =>
      runOperation(i18next.t('projects.saveProject', 'save project'), async () => {
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

  const handleOpenProjectPath = useCallback(
    (path) => runOperation(i18next.t('projects.openProject', 'open project'), () => loadProjectPath(path)),
    [loadProjectPath, runOperation],
  )

  const createProject = useCallback(
    () =>
      runOperation(i18next.t('projects.createProject', 'create project'), async () => {
        await createNewProject({ clearImportedVideo })
        markNew()
        return true
      }),
    [clearImportedVideo, markNew, runOperation],
  )

  const handleStartupNewProject = useCallback(async () => {
    if (await createProject()) setStartupDialogOpen(false)
  }, [createProject])

  const handleStartupOpenProject = useCallback(
    async (path) => {
      setStartupOpeningPath(path)
      setStartupDialogOpen(false)
      try {
        if (!(await handleOpenProjectPath(path))) setStartupDialogOpen(true)
      } finally {
        setStartupOpeningPath(null)
      }
    },
    [handleOpenProjectPath],
  )

  // Create new project — asks to save unsaved changes before discarding them
  const confirmProjectTransition = useCallback(
    async (intent) => {
      if (status === 'Saved') return true

      setConfirmationIntent(intent)
      const action = await requestProjectConfirm()
      if (action === 'cancel') return false
      if (action === 'save') return save(false)
      return true
    },
    [requestProjectConfirm, save, status],
  )

  const handleNewProject = useCallback(async () => {
    if (!(await confirmProjectTransition('new'))) return false
    return createProject()
  }, [confirmProjectTransition, createProject])

  const handleCloseRequest = useCallback(
    () => (isProjectConfirmOpen ? Promise.resolve(false) : confirmProjectTransition('close')),
    [confirmProjectTransition, isProjectConfirmOpen],
  )

  const closingApplication = confirmationIntent === 'close'
  const unsavedProjectDialog = {
    open: isProjectConfirmOpen,
    title: closingApplication ? i18next.t('projects.unsavedChangesTitle') : i18next.t('projects.createNewProject', 'Create New Project'),
    description: closingApplication
      ? i18next.t(
          'projects.unsavedChangesBeforeExit',
          'Your project has unsaved changes. Do you want to save the changes or close OVRLEY without saving?',
        )
      : i18next.t('projects.unsavedChanges', 'Your project has unsaved changes. Save them or discard them.'),
    discardLabel: closingApplication ? i18next.t('projects.closeWithoutSaving', 'Close Without Saving') : 'New Project',
    onCancel: () => answerProjectConfirm('cancel'),
    onSave: () => answerProjectConfirm('save'),
    onDiscard: () => answerProjectConfirm('discard'),
  }

  return {
    busy,
    handleCloseRequest,
    handleNewProject,
    handleOpenProject,
    handleSaveProject,
    handleSaveProjectAs,
    loadedProjectPath,
    loadingProject: activeOperation === i18next.t('projects.openProject', 'open project'),
    missingSourceDialog,
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
    unsavedProjectDialog,
  }
}
