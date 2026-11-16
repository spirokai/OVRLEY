/**
 * Canonical imported-video entities and their initial edits.
 *
 * An entity is created from probed metadata once, at import ingress. It is
 * registered with the preview server only when it joins the collection, so an
 * entity under review in the import dialog has no `preview` yet.
 *
 * A project video whose file cannot be found is `missing`: it is created from the
 * metadata saved with the project, keeps its place and edits, and has no preview
 * and no thumbnail until the user locates the file.
 */

import { frameCountForDuration } from './frame-grid'
import { FIRST_SEGMENT_KEY } from './video-segments'

const TELEMETRY_STATUSES = new Set(['pending', 'ready', 'absent', 'failed'])

function requirePositiveInteger(value, label) {
  if (!Number.isInteger(value) || value <= 0) throw new Error(`Imported video metadata contains an invalid ${label}`)
}

function greatestCommonDivisor(left, right) {
  return right === 0 ? left : greatestCommonDivisor(right, left % right)
}

function requireRotation(metadata) {
  const rotation = metadata.rotationDegrees === null || metadata.rotationDegrees === undefined ? 0 : Number(metadata.rotationDegrees)
  if (!Number.isFinite(rotation)) {
    throw new Error('Imported video metadata contains an invalid rotation')
  }

  const normalizedRotation = ((rotation % 360) + 360) % 360
  if (![0, 90, 180, 270].includes(normalizedRotation)) {
    throw new Error('Imported video metadata contains an unsupported rotation')
  }
  return normalizedRotation
}

function displayResolutionForImportedVideo(metadata, rotationDegrees) {
  const resolution = metadata.resolution
  if (!resolution) {
    throw new Error('Imported video metadata must include a resolution')
  }

  const width = Number(resolution.width)
  const height = Number(resolution.height)
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    throw new Error('Imported video metadata contains an invalid resolution')
  }

  if (rotationDegrees === 90 || rotationDegrees === 270) {
    return { width: height, height: width }
  }

  return { width, height }
}

/**
 * Validates a settled or pending embedded-telemetry result.
 * @param {{status: string, activity: object|null}} telemetry Telemetry result.
 */
export function requireTelemetry({ status, activity }) {
  if (!TELEMETRY_STATUSES.has(status)) throw new Error(`Invalid video telemetry status: ${status}`)
  if ((status === 'ready') !== (activity !== null)) throw new Error('Only ready video telemetry carries an activity')
}

function createVideoId() {
  return globalThis.crypto.randomUUID()
}

/**
 * Validates probed source metadata once at import ingress and creates a canonical video entity
 * without a preview registration. Duration is stored on the exact frame grid:
 * `frameCount × fps.den / fps.num`. `resolution` is display-oriented and
 * `rotationDegrees` is the display rotation, normalized to 0, 90, 180 or 270. User edits (placement, cuts, segments) live in
 * `videoEdits` keyed by the entity ID.
 * @param {object} metadata Probed source metadata.
 * @param {object} options Entity identity.
 * @param {number} options.importOrder Monotonic import order within the session.
 * @param {Array<string>} options.previewWarnings Warnings of the preview probe.
 * @param {string|null} options.thumbnailUrl Card thumbnail (`data:` URL), or null when no frame could be decoded.
 * @param {string} [options.id] Stable entity ID; generated when absent.
 * @returns {object} Imported video entity with `preview: null` and pending telemetry.
 */
export function createImportedVideo(metadata, { importOrder, previewWarnings, thumbnailUrl, id = createVideoId() }) {
  if (typeof metadata.path !== 'string' || metadata.path.length === 0) {
    throw new Error('Imported video metadata must include a path')
  }
  if (!Number.isFinite(metadata.duration) || metadata.duration <= 0) {
    throw new Error('Imported video metadata contains an invalid duration')
  }
  requirePositiveInteger(metadata.fpsNum, 'frame rate numerator')
  requirePositiveInteger(metadata.fpsDen, 'frame rate denominator')
  if (typeof metadata.hasAudio !== 'boolean') {
    throw new Error('Imported video metadata must report audio presence')
  }
  if (!Array.isArray(previewWarnings)) throw new Error('Imported video requires its preview warnings')
  if (thumbnailUrl !== null && typeof thumbnailUrl !== 'string') throw new Error('Imported video thumbnail must be a URL or null')

  const divisor = greatestCommonDivisor(metadata.fpsNum, metadata.fpsDen)
  const fps = { num: metadata.fpsNum / divisor, den: metadata.fpsDen / divisor }
  // A positive duration always has at least one frame.
  const frameCount = frameCountForDuration(metadata.duration, fps)
  const rotationDegrees = requireRotation(metadata)

  return {
    id,
    path: metadata.path,
    importOrder,
    durationSeconds: (frameCount * fps.den) / fps.num,
    frameCount,
    fps,
    resolution: displayResolutionForImportedVideo(metadata, rotationDegrees),
    rotationDegrees,
    hasAudio: metadata.hasAudio,
    codecName: metadata.codecName ?? null,
    codecLongName: metadata.codecLongName ?? null,
    bitRate: metadata.bitRate ?? null,
    cameraType: metadata.cameraType ?? null,
    cameraModel: metadata.cameraModel ?? null,
    creationTime: metadata.creationTime ?? null,
    timeSource: metadata.timeSource ?? null,
    detectedCreationTime: metadata.creationTime ?? null,
    detectedTimeSource: metadata.timeSource ?? null,
    previewWarnings: [...previewWarnings],
    missing: false,
    thumbnailUrl,
    preview: null,
    syncWarning: null,
    telemetry: { status: 'pending', activity: null },
  }
}

/**
 * Creates the entity of a project video whose file cannot be found, from the
 * metadata saved with the project (validated at the native project boundary).
 * It has no preview and no thumbnail, and no embedded telemetry can be read.
 * @param {object} metadata Saved metadata (`createVideoMetadataSnapshot`).
 * @param {{id: string, path: string, importOrder: number}} identity Saved identity and last known path.
 * @returns {object} Missing video entity.
 */
export function createMissingVideo(metadata, { id, path, importOrder }) {
  const fps = { ...metadata.fps }
  return {
    id,
    path,
    importOrder,
    durationSeconds: (metadata.frameCount * fps.den) / fps.num,
    frameCount: metadata.frameCount,
    fps,
    resolution: { ...metadata.resolution },
    rotationDegrees: metadata.rotationDegrees,
    hasAudio: metadata.hasAudio,
    codecName: metadata.codecName,
    codecLongName: metadata.codecLongName,
    bitRate: metadata.bitRate,
    cameraType: metadata.cameraType,
    cameraModel: metadata.cameraModel,
    creationTime: metadata.creationTime,
    timeSource: metadata.timeSource,
    detectedCreationTime: metadata.creationTime,
    detectedTimeSource: metadata.timeSource,
    previewWarnings: [],
    missing: true,
    thumbnailUrl: null,
    preview: null,
    syncWarning: null,
    telemetry: { status: 'failed', activity: null },
  }
}

/**
 * Describes a video as saved with the project, so that it can be shown and kept on
 * the timeline when its file cannot be found (`createMissingVideo`).
 * @param {object} video Imported video entity.
 * @returns {object} Saved metadata.
 */
export function createVideoMetadataSnapshot(video) {
  return {
    frameCount: video.frameCount,
    fps: { ...video.fps },
    resolution: { ...video.resolution },
    rotationDegrees: video.rotationDegrees,
    hasAudio: video.hasAudio,
    codecName: video.codecName,
    codecLongName: video.codecLongName,
    bitRate: video.bitRate,
    cameraType: video.cameraType,
    cameraModel: video.cameraModel,
    creationTime: video.detectedCreationTime,
    timeSource: video.detectedTimeSource,
  }
}

/**
 * Attaches the preview-server registration of a video joining the collection.
 * @param {object} video Imported video entity without a preview.
 * @param {{importId: string, previewUrl: string}} registration Preview registration.
 * @returns {object} Entity with `preview: {importId, url}`.
 */
export function withPreviewRegistration(video, registration) {
  if (video.preview !== null) throw new Error(`Video is already registered for preview: ${video.path}`)
  if (typeof registration.importId !== 'string' || typeof registration.previewUrl !== 'string') {
    throw new Error('Imported video requires a preview registration')
  }
  return { ...video, preview: { importId: registration.importId, url: registration.previewUrl } }
}

/**
 * Creates the user edits of a newly imported video. `offsetSource` is `manual`
 * once the start was set by hand, which re-detection keeps. `placementAnchor`
 * places a clip whose time cannot be determined directly before or after another clip.
 * Cuts and segments are described in `video-segments`.
 * @returns {{offsetSeconds: number, offsetSource: 'auto'|'manual', cuts: Array<{id: string, frame: number}>, segments: object, placementAnchor: {videoId: string, side: 'before'|'after'}|null}}
 *   Initial edits.
 */
export function createVideoEdits() {
  return {
    offsetSeconds: 0,
    offsetSource: 'auto',
    cuts: [],
    segments: { [FIRST_SEGMENT_KEY]: { excluded: false, transitionIn: null, skipOverlay: false } },
    placementAnchor: null,
    // Apply Timezone of the clip's camera clock; null until it is inferred or chosen.
    timezoneMode: null,
  }
}

/**
 * Returns the next import order for videos added to an existing collection.
 * @param {Array<object>} videos Current imported videos.
 * @returns {number} Next monotonic import order.
 */
export function nextImportOrder(videos) {
  return videos.reduce((next, video) => Math.max(next, video.importOrder + 1), 0)
}
