import * as backend from '@/api/backend'
import { replaceEditorDocument } from '@/features/undo-redo/undoHistory'
import { createDurableEditorState } from '@/lib/widget/editor-state'
import useStore from '@/store/useStore'
import { applyNewProjectState, applyPreparedProjectState } from './utils/projectHydration'
import { createProjectSnapshot, stringifyProject } from './utils/projectSnapshot'

/**
 * Loads project dependencies through their owners, then applies project-owned settings.
 * @param {object} options Project path, source resolver, and media-owner operations.
 * @param {function} [options.onSetBackgroundMode] Shell setter for the editor background mode.
 * @returns {Promise<object|null>} Loaded project, or null when source recovery is cancelled.
 */
export async function loadProject({ path, resolveProjectSources, prepareActivityPath, prepareVideoPath, onSetBackgroundMode }) {
  const { project: loadedProject, resolvedSources } = await backend.readProjectFile(path)
  const project = {
    ...loadedProject,
    editor: createDurableEditorState(loadedProject.editor),
  }
  const sources = await resolveProjectSources(resolvedSources)
  if (!sources) return null

  const sourceLoadResults = await Promise.allSettled([
    sources.activityPath ? prepareActivityPath(sources.activityPath) : Promise.resolve(null),
    sources.videoPath ? prepareVideoPath(sources.videoPath) : Promise.resolve(null),
  ])
  const failedSourceLoad = sourceLoadResults.find((result) => result.status === 'rejected')
  if (failedSourceLoad) throw failedSourceLoad.reason

  const activity = sourceLoadResults[0].value
  const preparedVideo = sourceLoadResults[1].value
  let video = null
  if (preparedVideo) {
    const registration = await backend.registerPreviewVideo(preparedVideo.path)
    video = {
      ...preparedVideo,
      importedVideoState: {
        ...preparedVideo.importedVideoState,
        importedVideoImportId: registration.importId,
        importedVideoPreviewUrl: registration.previewUrl,
      },
    }
  } else {
    await backend.clearPreviewVideo()
  }

  replaceEditorDocument(useStore, () => applyPreparedProjectState(useStore, project, { activity, video }))
  if (video) onSetBackgroundMode?.('video')
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
