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

// Each thumbnail decodes a frame in its own FFmpeg process; a large import runs a few at a time.
const THUMBNAIL_CONCURRENCY = 4
let runningThumbnails = 0
const queuedThumbnails = []

async function renderThumbnail(path) {
  if (runningThumbnails >= THUMBNAIL_CONCURRENCY) await new Promise((resolve) => queuedThumbnails.push(resolve))
  runningThumbnails += 1
  try {
    return await backend.getVideoThumbnail(path)
  } finally {
    runningThumbnails -= 1
    queuedThumbnails.shift()?.()
  }
}

/**
 * Probes one video file and renders its thumbnail, then creates its entity
 * without a preview registration.
 * @param {string} path Absolute video path.
 * @param {{importOrder: number, id?: string}} identity Entity identity.
 * @returns {Promise<object>} Imported video entity.
 */
export async function probeVideo(path, identity) {
  if (!isVideoPath(path)) throw new Error(i18next.t('video-preview.unsupportedVideoFile', 'Not a supported video file (MP4, MOV, MKV).'))
  const [prepared, thumbnailUrl] = await Promise.all([backend.preparePreviewVideo(path), renderThumbnail(path)])
  return createImportedVideo(prepared.metadata, { ...identity, previewWarnings: prepared.warnings, thumbnailUrl })
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
 * Missing videos have no registration.
 * @param {Array<object>} videos Registered or missing entities.
 */
export async function releaseVideoPreviews(videos) {
  for (const video of videos) if (!video.missing) await backend.removePreviewVideo(video.preview.importId)
}

/** Removes every preview registration, transition proxies included, and forgets the proxies. */
export async function clearVideoPreviews() {
  await backend.clearPreviewVideos()
  useStore.getState().clearTransitionProxies()
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
 * Relinks a missing video to the file the user located: probes the file under the
 * video's identity, registers its preview, and replaces the missing entity, keeping
 * its edits (cuts past the end of a shorter file are dropped). The video's earlier
 * edits leave the undo history, and its telemetry is read in the background.
 * @param {string} videoId Missing video.
 * @param {string} path Located file.
 * @returns {Promise<void>} Completion of the relink.
 */
export async function locateMissingVideo(videoId, path) {
  const missing = useStore.getState().importedVideos.find((video) => video.id === videoId)
  if (missing === undefined || !missing.missing) throw new Error(`Video is not missing: ${videoId}`)
  const [video] = await registerVideoPreviews([await probeVideo(path, { id: videoId, importOrder: missing.importOrder })])
  try {
    changeVideoCollection(useStore, () => useStore.getState().relinkVideo(video), { replacedVideoIds: [videoId] })
  } catch (error) {
    await releaseVideoPreviews([video])
    throw error
  }
  void queueTelemetryExtraction([video])
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
  const added = await registerVideoPreviews(collection.importedVideos.filter((video) => !video.missing && video.preview === null))
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
