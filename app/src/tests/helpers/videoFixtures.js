/**
 * Canonical imported-video fixtures for tests.
 */

import { requireTelemetry, createImportedVideo, createVideoEdits, withPreviewRegistration } from '@/lib/video-program/imported-video'

/**
 * Creates a validated imported-video entity through the production ingress,
 * registered for preview unless `registered` is false.
 * @param {object} [options] Fixture overrides.
 * @returns {object} Imported video entity.
 */
export function createTestVideo({
  id = 'video-1',
  path = '/videos/ride.mp4',
  durationSeconds = 60,
  fps = { num: 30, den: 1 },
  width = 1920,
  height = 1080,
  rotationDegrees = 0,
  hasAudio = true,
  creationTime = null,
  timeSource = null,
  codecName = 'h264',
  importOrder = 0,
  previewWarnings = [],
  telemetry = { status: 'pending', activity: null },
  registered = true,
} = {}) {
  requireTelemetry(telemetry)
  const video = createImportedVideo(
    {
      path,
      duration: durationSeconds,
      fpsNum: fps.num,
      fpsDen: fps.den,
      resolution: { width, height },
      rotationDegrees,
      hasAudio,
      creationTime,
      timeSource,
      codecName,
    },
    { id, importOrder, previewWarnings },
  )
  const withTelemetry = { ...video, telemetry }
  return registered
    ? withPreviewRegistration(withTelemetry, { importId: `import-${id}`, previewUrl: `http://127.0.0.1:4321/video/import-${id}` })
    : withTelemetry
}

/**
 * Store fields for an imported-video collection placed at explicit offsets,
 * bypassing automatic synchronization.
 * @param {Array<object>} videos Imported video entities.
 * @param {Object<string, number>} [offsets] Offset seconds keyed by video ID; 0 when absent.
 * @returns {{importedVideos: Array<object>, videoEdits: object}} Partial store state.
 */
export function videoStoreState(videos, offsets = {}) {
  return {
    importedVideos: videos,
    videoEdits: Object.fromEntries(videos.map((video) => [video.id, { ...createVideoEdits(), offsetSeconds: offsets[video.id] ?? 0 }])),
  }
}
