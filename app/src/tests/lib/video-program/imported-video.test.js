import { describe, expect, test } from 'vitest'
import { createImportedVideo, createVideoEdits, nextImportOrder, requireTelemetry, withPreviewRegistration } from '@/lib/video-program/imported-video'

function metadata(overrides = {}) {
  return { path: '/videos/ride.mp4', duration: 60, fpsNum: 30, fpsDen: 1, resolution: { width: 1920, height: 1080 }, hasAudio: true, ...overrides }
}

describe('createImportedVideo ingress', () => {
  test('stores exact reduced FPS, a frame-exact duration, and display-oriented resolution, without a preview yet', () => {
    const video = createImportedVideo(metadata({ duration: 10.0103, fpsNum: 60000, fpsDen: 2002, rotationDegrees: 90 }), {
      id: 'v',
      importOrder: 3,
      previewWarnings: ['slow seeking'],
    })
    expect(video.fps).toEqual({ num: 30000, den: 1001 })
    // 300.09 frames: the partial last frame counts, as in the native renderer.
    expect(video.frameCount).toBe(301)
    expect(video.durationSeconds).toBeCloseTo((301 * 1001) / 30000, 12)
    expect(video.resolution).toEqual({ width: 1080, height: 1920 })
    expect(video).toMatchObject({
      id: 'v',
      importOrder: 3,
      preview: null,
      previewWarnings: ['slow seeking'],
      syncWarning: null,
      telemetry: { status: 'pending', activity: null },
    })
  })

  test.each([
    ['a missing path', { path: '' }, 'must include a path'],
    ['a non-positive duration', { duration: 0 }, 'invalid duration'],
    ['a missing FPS numerator', { fpsNum: undefined }, 'invalid frame rate numerator'],
    ['a fractional FPS denominator', { fpsDen: 1.5 }, 'invalid frame rate denominator'],
    ['unknown audio presence', { hasAudio: undefined }, 'audio presence'],
  ])('rejects %s', (_label, overrides, message) => {
    expect(() => createImportedVideo(metadata(overrides), { importOrder: 0, previewWarnings: [] })).toThrow(message)
  })

  test('counts frames like the native renderer, snapping container jitter to the frame boundary', () => {
    const frames = (duration, fpsNum = 30, fpsDen = 1) =>
      createImportedVideo(metadata({ duration, fpsNum, fpsDen }), { importOrder: 0, previewWarnings: [] }).frameCount
    expect(frames(10)).toBe(300)
    expect(frames(10.000001)).toBe(300)
    expect(frames(10.01)).toBe(301)
    expect(frames(0.001)).toBe(1)
    expect(frames(10.01, 30000, 1001)).toBe(300)
  })

  test('attaches one preview registration when the video joins the collection', () => {
    const video = createImportedVideo(metadata(), { importOrder: 0, previewWarnings: [] })
    const registered = withPreviewRegistration(video, { importId: 'import-1', previewUrl: 'http://127.0.0.1:1/video/import-1' })
    expect(registered.preview).toEqual({ importId: 'import-1', url: 'http://127.0.0.1:1/video/import-1' })
    expect(() => withPreviewRegistration(registered, { importId: 'import-2', previewUrl: 'x' })).toThrow('already registered')
    expect(() => withPreviewRegistration(video, { importId: 'import-1' })).toThrow('requires a preview registration')
  })

  test('accepts settled telemetry and rejects inconsistent telemetry', () => {
    expect(() => requireTelemetry({ status: 'ready', activity: {} })).not.toThrow()
    expect(() => requireTelemetry({ status: 'absent', activity: {} })).toThrow('Only ready video telemetry carries an activity')
    expect(() => requireTelemetry({ status: 'done', activity: null })).toThrow('Invalid video telemetry status')
  })

  test('new edits are automatic and import order continues after the collection', () => {
    expect(createVideoEdits()).toEqual({ offsetSeconds: 0, offsetSource: 'auto', deletedParts: [], transitionIn: null, placementAnchor: null })
    expect(nextImportOrder([])).toBe(0)
    expect(nextImportOrder([{ importOrder: 4 }, { importOrder: 1 }])).toBe(5)
  })
})
