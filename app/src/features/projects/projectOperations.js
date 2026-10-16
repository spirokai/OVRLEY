import * as backend from '@/api/backend'
import { replaceEditorDocument } from '@/features/undo-redo/undoHistory'
import { createDurableEditorState } from '@/lib/widget/editor-state'
import { createImportedVideo } from '@/store/slices/createVideoImportSlice'
import { attachRasterLoadResults } from '@/lib/widget/raster-resources'
import useStore from '@/store/useStore'
import { prepareDocumentFonts } from '@/lib/font-resources'
import { applyNewProjectState, applyPreparedProjectState } from './utils/projectHydration'
import { createProjectSnapshot, stringifyProject } from './utils/projectSnapshot'

async function removePreviewRegistrations(videos) {
  for (const video of videos) await backend.removePreviewVideo(video.preview.importId)
}

/**
 * Registers prepared project videos as entities that keep their saved identity.
 * Registrations already made are removed again when a later video fails.
 * @param {Array<object>} projectVideos Saved project videos in import order.
 * @param {Array<object|null>} preparedVideos Prepared videos aligned with `projectVideos`; null for skipped sources.
 * @returns {Promise<Array<object>>} Registered video entities.
 */
async function registerProjectVideos(projectVideos, preparedVideos) {
  const videos = []
  try {
    for (const [index, preparedVideo] of preparedVideos.entries()) {
      if (preparedVideo === null) continue
      const registration = await backend.registerPreviewVideo(preparedVideo.path)
      try {
        videos.push(
          createImportedVideo(preparedVideo.metadata, {
            id: projectVideos[index].id,
            importOrder: index,
            preview: { importId: registration.importId, url: registration.previewUrl, warnings: preparedVideo.previewWarnings },
            telemetry: preparedVideo.telemetry,
          }),
        )
      } catch (error) {
        await backend.removePreviewVideo(registration.importId)
        throw error
      }
    }
  } catch (error) {
    await removePreviewRegistrations(videos)
    throw error
  }
  return videos
}

/**
 * Loads project dependencies through their owners, then applies project-owned settings.
 * @param {object} options Project path, source resolver, and media-owner operations.
 * @param {function} [options.onSetBackgroundMode] Shell setter for the editor background mode.
 * @returns {Promise<{project: object, legacyBatchVideoFolder: string|null}|null>} Loaded project and, for a version 3
 *   batch project, the video folder to import; null when source recovery is cancelled.
 */
export async function loadProject({ path, resolveProjectSources, prepareActivityPath, prepareVideoPath, onSetBackgroundMode }) {
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
    ...sources.videoPaths.map((videoPath) => (videoPath ? prepareVideoPath(videoPath) : Promise.resolve(null))),
  ])
  const failedSourceLoad = sourceLoadResults.find((result) => result.status === 'rejected')
  if (failedSourceLoad) throw failedSourceLoad.reason

  const activity = sourceLoadResults[0].value
  const preparedVideos = sourceLoadResults.slice(2).map((result) => result.value)
  const previousImportIds = useStore.getState().importedVideos.map((importedVideo) => importedVideo.preview.importId)
  const videos = await registerProjectVideos(project.sources.videos, preparedVideos)
  if (videos.length === 0) await backend.clearPreviewVideos()

  try {
    replaceEditorDocument(useStore, () => applyPreparedProjectState(useStore, hydratedProject, { activity, videos }))
  } catch (error) {
    await removePreviewRegistrations(videos)
    throw error
  }
  if (videos.length > 0) for (const importId of previousImportIds) await backend.removePreviewVideo(importId)
  if (videos.length > 0) onSetBackgroundMode?.('video')
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
