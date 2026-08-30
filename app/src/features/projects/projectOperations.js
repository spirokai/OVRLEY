import * as backend from '@/api/backend'
import { replaceEditorDocument } from '@/features/undo-redo/undoHistory'
import useStore from '@/store/useStore'
import { applyNewProjectState, applyProjectOwnedState } from './utils/projectHydration'
import { createProjectSnapshot, stringifyProject } from './utils/projectSnapshot'

/**
 * Loads project dependencies through their owners, then applies project-owned settings.
 * @param {object} options Project path, source resolver, and media-owner operations.
 * @returns {Promise<object|null>} Loaded project, or null when source recovery is cancelled.
 */
export async function loadProject({ path, resolveProjectSources, loadActivityPath, loadVideoPath, clearImportedVideo }) {
  const { project, resolvedSources } = await backend.readProjectFile(path)
  const sources = await resolveProjectSources(resolvedSources)
  if (!sources) return null

  const sourceLoadResults = await Promise.allSettled([
    sources.activityPath
      ? loadActivityPath(sources.activityPath)
      : Promise.resolve(useStore.getState().clearActivityFile({ restoreVideoTelemetry: false })),
    sources.videoPath ? loadVideoPath(sources.videoPath) : useStore.getState().importedVideoPath ? clearImportedVideo() : Promise.resolve(),
  ])
  const failedSourceLoad = sourceLoadResults.find((result) => result.status === 'rejected')
  if (failedSourceLoad) throw failedSourceLoad.reason

  replaceEditorDocument(useStore, () => applyProjectOwnedState(useStore, project))
  return project
}

/**
 * Resets the active session while preserving a valid loaded-template baseline.
 * @param {object} options Project media-owner operations.
 * @returns {Promise<void>} Completion promise.
 */
export async function createNewProject({ clearImportedVideo }) {
  const state = useStore.getState()
  const templateSource = state.loadedTemplateSource
  const templateState = state.lastSavedTemplateState
  if (templateSource && !templateState) throw new Error('the loaded template has no saved widget state')

  await clearImportedVideo()
  replaceEditorDocument(useStore, () => applyNewProjectState(useStore, { templateSource, templateState }))
}

/**
 * Writes the current canonical project snapshot and returns that snapshot.
 * @param {string} path Absolute project destination path.
 * @returns {Promise<object>} Written canonical project snapshot.
 */
export async function saveProject(path) {
  const project = createProjectSnapshot(useStore.getState(), path)
  await backend.writeProjectFile(path, stringifyProject(project))
  return project
}
