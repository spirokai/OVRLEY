/**
 * The one way videos join the collection, for an interactive import and for a
 * loaded project: probe each file without touching the preview server or the
 * store, review (import dialog) or validate (project) the result, register the
 * previews of the videos that join, commit, then extract embedded telemetry in
 * the background.
 */

import * as backend from '@/api/backend'
import { changeVideoCollection, runWithoutEditorHistory } from '@/features/undo-redo/undoHistory'
import { normalizeUpdateRateForFps } from '@/lib/update-rate'
import { createImportedVideo, withPreviewRegistration } from '@/lib/video-program/imported-video'
import { sortVideosByTimeline, joinVideoEdits } from '@/lib/video-program/video-collection'
import useStore from '@/store/useStore'
import i18next from 'i18next'

const VIDEO_EXTENSIONS = new Set(['mp4', 'mov', 'mkv'])

/**
 * Whether a path names a supported video file.
 * @param {string} path Absolute path.
 * @returns {boolean} Whether the extension is MP4, MOV, or MKV.
 */
export function isVideoPath(path) {
  return VIDEO_EXTENSIONS.has(path.split('.').pop()?.toLowerCase() ?? '')
}

/**
 * Probes one video file and creates its entity, without a preview registration.
 * @param {string} path Absolute video path.
 * @param {{importOrder: number, id?: string}} identity Entity identity.
 * @returns {Promise<object>} Imported video entity.
 */
export async function probeVideo(path, identity) {
  if (!isVideoPath(path)) throw new Error(i18next.t('video-preview.unsupportedVideoFile', 'Not a supported video file (MP4, MOV, MKV).'))
  const prepared = await backend.preparePreviewVideo(path)
  return createImportedVideo(prepared.metadata, { ...identity, previewWarnings: prepared.warnings })
}

/**
 * Registers videos with the preview server. Registrations already made are removed again when one fails.
 * @param {Array<object>} videos Entities without a preview.
 * @returns {Promise<Array<object>>} Entities with their preview registration.
 */
export async function registerVideoPreviews(videos) {
  const registered = []
  try {
    for (const video of videos) registered.push(withPreviewRegistration(video, await backend.registerPreviewVideo(video.path)))
  } catch (error) {
    await releaseVideoPreviews(registered)
    throw error
  }
  return registered
}

/**
 * Removes the preview registrations of videos that left (or never joined) the collection.
 * @param {Array<object>} videos Registered entities.
 */
export async function releaseVideoPreviews(videos) {
  for (const video of videos) await backend.removePreviewVideo(video.preview.importId)
}

async function extractTelemetryResult(path) {
  try {
    const response = await backend.extractVideoTelemetry(path)
    return response?.parsed_activity ? { status: 'ready', activity: response.parsed_activity } : { status: 'absent', activity: null }
  } catch (error) {
    console.warn('MP4 telemetry extraction failed (non-fatal):', error)
    return { status: 'failed', activity: null }
  }
}

// Extraction is heavy, so the application runs one at a time across every import.
let telemetryQueue = Promise.resolve()

/**
 * Queues embedded telemetry extraction for videos that joined the collection, in
 * timeline order. Results for videos removed meanwhile are discarded; missing or
 * failed telemetry is a valid state and never prevents playback.
 * @param {Array<object>} videos Committed entities.
 * @returns {Promise<void>} Completion of this batch of the queue.
 */
export function queueTelemetryExtraction(videos) {
  const ordered = sortVideosByTimeline(joinVideoEdits(videos, useStore.getState().videoEdits))
  telemetryQueue = telemetryQueue
    .then(async () => {
      for (const video of ordered) {
        if (!useStore.getState().importedVideos.some((candidate) => candidate.id === video.id)) continue
        const telemetry = await extractTelemetryResult(video.path)
        runWithoutEditorHistory(useStore, () => useStore.getState().setVideoTelemetry(video.id, telemetry))
      }
    })
    // Later imports must still get their telemetry after an unexpected failure.
    .catch((error) => useStore.getState().setErrorMessage(`Video telemetry failed: ${error instanceof Error ? error.message : String(error)}`))
  return telemetryQueue
}

/** Sizes the scene and render FPS to the format of the first videos. */
function adoptVideoFormat(format) {
  const { config, renderSettings, setConfig, setRenderFpsAndUpdateRate } = useStore.getState()
  if (!config?.scene) throw new Error('Cannot import video without an active template scene')
  setConfig({ ...config, scene: { ...config.scene, width: format.resolution.width, height: format.resolution.height } })
  const fps = Math.round(format.fps.num / format.fps.den)
  setRenderFpsAndUpdateRate(fps, normalizeUpdateRateForFps(fps, renderSettings.widgetUpdateRate))
}

/**
 * Commits a reviewed import: registers the previews of its new videos, replaces the
 * collection and the transition defaults in one change, releases the previews of
 * videos removed from the project, and queues telemetry of the new videos.
 * @param {{importedVideos: Array<object>, videoEdits: object, videoSyncTimezoneMode: 'utc'|'local'|null}} collection Reviewed
 *   collection; its new videos have no preview yet.
 * @param {{type: string, durationSeconds: number, splitGapThresholdSeconds: number}} transitionDefaults Reviewed defaults.
 * @param {{resolution: object, fps: object}|null} adoptedFormat Format the scene adopts (the first import), or null.
 * @returns {Promise<Array<object>>} The new videos as committed.
 */
export async function commitReviewedImport(collection, transitionDefaults, adoptedFormat) {
  const added = await registerVideoPreviews(collection.importedVideos.filter((video) => video.preview === null))
  const addedById = new Map(added.map((video) => [video.id, video]))
  const keptIds = new Set(collection.importedVideos.map((video) => video.id))
  const removed = useStore.getState().importedVideos.filter((video) => !keptIds.has(video.id))
  try {
    changeVideoCollection(useStore, () => {
      const state = useStore.getState()
      state.commitVideoCollection({
        ...collection,
        importedVideos: collection.importedVideos.map((video) => addedById.get(video.id) ?? video),
      })
      state.setTransitionDefaults(transitionDefaults)
      if (adoptedFormat !== null) adoptVideoFormat(adoptedFormat)
    })
  } catch (error) {
    await releaseVideoPreviews(added)
    throw error
  }
  await releaseVideoPreviews(removed)
  void queueTelemetryExtraction(added)
  return added
}
