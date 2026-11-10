import * as backend from '@/api/backend'
import { replaceEditorDocument } from '@/features/undo-redo/undoHistory'
import {
  clearVideoPreviews,
  probeVideo,
  queueTelemetryExtraction,
  registerVideoPreviews,
  releaseVideoPreviews,
} from '@/features/video-preview/videoImportOperations'
import { createMissingVideo, createVideoMetadataSnapshot } from '@/lib/video-program/imported-video'
import { createDurableEditorState } from '@/lib/template/template-state'
import { attachRasterLoadResults } from '@/lib/widget/widget-config'
import useStore from '@/store/useStore'
import { prepareDocumentFonts } from '@/lib/font-resources'
import { applyNewProjectState, applyPreparedProjectState, createProjectManualVideoSync, createProjectVideoEdits } from './utils/projectHydration'
import { createProjectSnapshot, stringifyProject } from './utils/projectSnapshot'

/**
 * Loads one saved video: a found file is probed (with its thumbnail); a missing one
 * is created from the metadata saved with it. A missing video of an older project
 * has no saved metadata and is left out.
 * @param {object} projectVideo Saved project video.
 * @param {string|null} path Resolved file, or null when the file is missing.
 * @param {string} savedPath The saved location, kept by a missing video.
 * @param {number} importOrder Position in the saved collection.
 * @returns {Promise<object|null>} Video entity, or null when left out.
 */
async function loadProjectVideo(projectVideo, path, savedPath, importOrder) {
  if (path !== null) return probeVideo(path, { id: projectVideo.id, importOrder })
  if (projectVideo.metadata === null) return null
  return createMissingVideo(projectVideo.metadata, { id: projectVideo.id, path: savedPath, importOrder })
}

/**
 * Loads a project through the import pipeline: every found video is probed and the
 * saved edits are fitted to the loaded videos before anything is registered or
 * committed, so a failure leaves the open session untouched. Videos whose files
 * cannot be found join the collection as missing.
 * @param {object} options Project path, source resolver, and media-owner operations.
 * @param {function} [options.onSetBackgroundMode] Shell setter for the editor background mode.
 * @returns {Promise<{project: object, legacyBatchVideoFolder: string|null}|null>} Saved baseline of the loaded project
 *   (its videos described as loaded) and, for a version 3 batch project, the video folder it rendered; null when
 *   source recovery is cancelled.
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
    ...project.sources.videos.map((projectVideo, index) =>
      loadProjectVideo(projectVideo, sources.videoPaths[index], resolvedSources.videoPaths[index], index),
    ),
  ])
  const failedSourceLoad = sourceLoadResults.find((result) => result.status === 'rejected')
  if (failedSourceLoad) throw failedSourceLoad.reason

  const activity = sourceLoadResults[0].value
  const loadedVideos = sourceLoadResults
    .slice(2)
    .map((result) => result.value)
    .filter((video) => video !== null)
  const videoEdits = createProjectVideoEdits(loadedVideos, project.sources.videos)
  const manualVideoSync = createProjectManualVideoSync(loadedVideos, project.sync.manual)

  const previousVideos = useStore.getState().importedVideos
  const registered = await registerVideoPreviews(loadedVideos.filter((video) => !video.missing))
  const registeredById = new Map(registered.map((video) => [video.id, video]))
  const videos = loadedVideos.map((video) => registeredById.get(video.id) ?? video)
  const collection = { importedVideos: videos, videoEdits, videoSyncTimezoneMode: project.sync.videoTimezoneMode }
  try {
    replaceEditorDocument(useStore, () => applyPreparedProjectState(useStore, hydratedProject, { activity, collection, manualVideoSync }))
  } catch (error) {
    await releaseVideoPreviews(registered)
    throw error
  }
  if (registered.length === 0) await clearVideoPreviews()
  else await releaseVideoPreviews(previousVideos)
  if (videos.length > 0) onSetBackgroundMode?.('video')
  void queueTelemetryExtraction(registered)
  return { project: describeLoadedVideos(project, videos), legacyBatchVideoFolder }
}

/**
 * Describes each loaded video as the session would save it: metadata learned from a
 * probe (an older project stored none) is not a change the user made.
 */
function describeLoadedVideos(project, videos) {
  const videosById = new Map(videos.map((video) => [video.id, video]))
  return {
    ...project,
    sources: {
      ...project.sources,
      videos: project.sources.videos.map((projectVideo) => {
        const video = videosById.get(projectVideo.id)
        return video === undefined ? projectVideo : { ...projectVideo, metadata: createVideoMetadataSnapshot(video) }
      }),
    },
  }
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
