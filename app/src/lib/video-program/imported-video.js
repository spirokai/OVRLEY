/**
 * Canonical imported-video entities and their initial edits.
 *
 * An entity is created from probed metadata once, at import ingress. It is
 * registered with the preview server only when it joins the collection, so an
 * entity under review in the import dialog has no `preview` yet.
 */

import { frameCountForDuration } from './frame-grid'

const TELEMETRY_STATUSES = new Set(['pending', 'ready', 'absent', 'failed'])

function requirePositiveInteger(value, label) {
  if (!Number.isInteger(value) || value <= 0) throw new Error(`Imported video metadata contains an invalid ${label}`)
}

function greatestCommonDivisor(left, right) {
  return right === 0 ? left : greatestCommonDivisor(right, left % right)
}

function displayResolutionForImportedVideo(metadata) {
  const resolution = metadata.resolution
  if (!resolution) {
    throw new Error('Imported video metadata must include a resolution')
  }

  const width = Number(resolution.width)
  const height = Number(resolution.height)
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    throw new Error('Imported video metadata contains an invalid resolution')
  }

  const rotation = metadata.rotationDegrees === null || metadata.rotationDegrees === undefined ? 0 : Number(metadata.rotationDegrees)
  if (!Number.isFinite(rotation)) {
    throw new Error('Imported video metadata contains an invalid rotation')
  }

  const normalizedRotation = ((rotation % 360) + 360) % 360
  if (![0, 90, 180, 270].includes(normalizedRotation)) {
    throw new Error('Imported video metadata contains an unsupported rotation')
  }

  if (normalizedRotation === 90 || normalizedRotation === 270) {
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
 * `frameCount × fps.den / fps.num`. User edits (placement, deleted parts, transitions) live in
 * `videoEdits` keyed by the entity ID.
 * @param {object} metadata Probed source metadata.
 * @param {object} options Entity identity.
 * @param {number} options.importOrder Monotonic import order within the session.
 * @param {Array<string>} options.previewWarnings Warnings of the preview probe.
 * @param {string} [options.id] Stable entity ID; generated when absent.
 * @returns {object} Imported video entity with `preview: null` and pending telemetry.
 */
export function createImportedVideo(metadata, { importOrder, previewWarnings, id = createVideoId() }) {
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

  const divisor = greatestCommonDivisor(metadata.fpsNum, metadata.fpsDen)
  const fps = { num: metadata.fpsNum / divisor, den: metadata.fpsDen / divisor }
  // A positive duration always has at least one frame.
  const frameCount = frameCountForDuration(metadata.duration, fps)

  return {
    id,
    path: metadata.path,
    importOrder,
    durationSeconds: (frameCount * fps.den) / fps.num,
    frameCount,
    fps,
    resolution: displayResolutionForImportedVideo(metadata),
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
    preview: null,
    syncWarning: null,
    telemetry: { status: 'pending', activity: null },
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
 * @returns {{offsetSeconds: number, offsetSource: 'auto'|'manual', deletedParts: Array<object>, transitionIn: object|null, placementAnchor: {videoId: string, side: 'before'|'after'}|null}}
 *   Initial edits.
 */
export function createVideoEdits() {
  return { offsetSeconds: 0, offsetSource: 'auto', deletedParts: [], transitionIn: null, placementAnchor: null }
}

/**
 * Returns the next import order for videos added to an existing collection.
 * @param {Array<object>} videos Current imported videos.
 * @returns {number} Next monotonic import order.
 */
export function nextImportOrder(videos) {
  return videos.reduce((next, video) => Math.max(next, video.importOrder + 1), 0)
}
