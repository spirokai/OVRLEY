import * as backend from '@/api/backend'
import { replaceEditorDocument } from '@/features/undo-redo/undoHistory'
import { probeVideo, queueTelemetryExtraction, registerVideoPreviews, releaseVideoPreviews } from '@/features/video-preview/videoImportOperations'
import { createDurableEditorState } from '@/lib/widget/editor-state'
import { attachRasterLoadResults } from '@/lib/widget/raster-resources'
import useStore from '@/store/useStore'
import { prepareDocumentFonts } from '@/lib/font-resources'
import { applyNewProjectState, applyPreparedProjectState, createProjectManualVideoSync, createProjectVideoEdits } from './utils/projectHydration'
import { createProjectSnapshot, stringifyProject } from './utils/projectSnapshot'

/**
 * Loads a project through the import pipeline: every source is probed and the
 * saved edits are validated against the probed videos before anything is
 * registered or committed, so a failure leaves the open session untouched.
 * @param {object} options Project path, source resolver, and media-owner operations.
 * @param {function} [options.onSetBackgroundMode] Shell setter for the editor background mode.
 * @returns {Promise<{project: object, legacyBatchVideoFolder: string|null}|null>} Loaded project and, for a version 3
 *   batch project, the video folder it rendered; null when source recovery is cancelled.
 */
export async function loadProject({ path, resolveProjectSources, prepareActivityPath, onSetBackgroundMode }) {
  const { project, resolvedSources, rasterLoadResults, legacyBatchVideoFolder } = await backend.readProjectFile(path)
  project.editor = createDurableEditorState(project.editor)
  const hydratedProject = {
    ...project,
    editor: { ...project.editor, config: attachRasterLoadResults(project.editor.config, rasterLoadResults) },
  }
  const sources = await resolveProjectSources(resolvedSources)
  if (!sources) return null

  const sourceLoadResults = await Promise.allSettled([
    sources.activityPath ? prepareActivityPath(sources.activityPath) : Promise.resolve(null),
    prepareDocumentFonts(project.editor),
    ...sources.videoPaths.map((videoPath, index) =>
      videoPath ? probeVideo(videoPath, { id: project.sources.videos[index].id, importOrder: index }) : Promise.resolve(null),
    ),
  ])
  const failedSourceLoad = sourceLoadResults.find((result) => result.status === 'rejected')
  if (failedSourceLoad) throw failedSourceLoad.reason

  const activity = sourceLoadResults[0].value
  const probedVideos = sourceLoadResults
    .slice(2)
    .map((result) => result.value)
    .filter((video) => video !== null)
  const videoEdits = createProjectVideoEdits(probedVideos, project.sources.videos)
  const manualVideoSync = createProjectManualVideoSync(probedVideos, project.sync.manual)

  const previousVideos = useStore.getState().importedVideos
  const videos = await registerVideoPreviews(probedVideos)
  const collection = { importedVideos: videos, videoEdits, videoSyncTimezoneMode: project.sync.videoTimezoneMode }
  try {
    replaceEditorDocument(useStore, () => applyPreparedProjectState(useStore, hydratedProject, { activity, collection, manualVideoSync }))
  } catch (error) {
    await releaseVideoPreviews(videos)
    throw error
  }
  if (videos.length === 0) {
    await backend.clearPreviewVideos()
    useStore.getState().clearTransitionProxies()
  } else {
    await releaseVideoPreviews(previousVideos)
    onSetBackgroundMode?.('video')
  }
  void queueTelemetryExtraction(videos)
  return { project, legacyBatchVideoFolder }
}

/**
 * Resets the active session while preserving a valid loaded-template baseline.
 * @param {object} options Project media-owner operations.
 * @returns {Promise<void>} Completion promise.
 */
export async function createNewProject({ clearImportedVideos }) {
  const state = useStore.getState()
  const templateSource = state.loadedTemplateSource
  const templateState = state.lastSavedTemplateState
  if (templateSource && !templateState) throw new Error('the loaded template has no saved widget state')

  if (templateState) await prepareDocumentFonts(templateState)
  await clearImportedVideos()
  replaceEditorDocument(useStore, () => applyNewProjectState(useStore, { templateSource, templateState }))
}

/**
 * Writes the current canonical project snapshot.
 * @param {string} path Absolute project destination path.
 * @returns {Promise<{project: object, backupPath: string|null}>} Written snapshot, and the backup of the
 *   older-format project it replaced, if any.
 */
export async function saveProject(path) {
  const state = useStore.getState()
  const project = createProjectSnapshot(state, path)
  const contents = stringifyProject(project)

  const { backupPath } = await backend.writeProjectFile(path, contents, state.config)
  return { project, backupPath }
}
