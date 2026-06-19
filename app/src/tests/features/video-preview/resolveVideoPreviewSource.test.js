/**
 * Regression tests for preview-source resolution helpers.
 *
 * The preview hook should resolve one clear source of truth for the `<video>`
 * element and keep out-of-range detection independent from the DOM effect.
 */

import { describe, expect, test, vi } from 'vitest'
import { isVideoPreviewOutOfRange, resolveVideoPreviewSource } from '@/features/video-preview/utils/videoPreviewSource'

describe('videoPreviewSource helpers', () => {
  test('prefers the imported preview URL when local HTTP preview mode is enabled', () => {
    const convertFileSrc = vi.fn((path) => `converted:${path}`)

    expect(
      resolveVideoPreviewSource({
        convertFileSrc,
        importedVideoPath: 'C:\\clips\\ride.mp4',
        importedVideoPreviewUrl: 'http://127.0.0.1:3210/preview/ride.mp4',
        useLocalHttpPreview: true,
      }),
    ).toBe('http://127.0.0.1:3210/preview/ride.mp4')

    expect(convertFileSrc).not.toHaveBeenCalled()
  })

  test('falls back to a file source when no preview URL exists', () => {
    const convertFileSrc = vi.fn((path) => `converted:${path}`)

    expect(
      resolveVideoPreviewSource({
        convertFileSrc,
        importedVideoPath: 'C:\\clips\\ride.mp4',
        importedVideoPreviewUrl: null,
        useLocalHttpPreview: true,
      }),
    ).toBe('converted:C:\\clips\\ride.mp4')

    expect(convertFileSrc).toHaveBeenCalledWith('C:\\clips\\ride.mp4')
  })

  test('keeps the local preview server source when the app is https', () => {
    const convertFileSrc = vi.fn((path) => `converted:${path}`)

    expect(
      resolveVideoPreviewSource({
        convertFileSrc,
        importedVideoPath: 'C:\\clips\\ride.mp4',
        importedVideoPreviewUrl: 'http://127.0.0.1:3210/video/abc',
        windowProtocol: 'https:',
        useLocalHttpPreview: true,
      }),
    ).toBe('http://127.0.0.1:3210/video/abc')

    expect(convertFileSrc).not.toHaveBeenCalled()
  })

  test('falls back to a file source for non-loopback http preview URLs when the app is https', () => {
    const convertFileSrc = vi.fn((path) => `converted:${path}`)

    expect(
      resolveVideoPreviewSource({
        convertFileSrc,
        importedVideoPath: 'C:\\clips\\ride.mp4',
        importedVideoPreviewUrl: 'http://192.168.1.20:3210/video/abc',
        windowProtocol: 'https:',
        useLocalHttpPreview: true,
      }),
    ).toBe('converted:C:\\clips\\ride.mp4')

    expect(convertFileSrc).toHaveBeenCalledWith('C:\\clips\\ride.mp4')
  })

  test('flags preview playback outside the imported video window', () => {
    expect(
      isVideoPreviewOutOfRange({
        selectedSecond: 4.99,
        videoDuration: 6,
        videoSyncOffsetSeconds: 5,
      }),
    ).toBe(true)

    expect(
      isVideoPreviewOutOfRange({
        selectedSecond: 11,
        videoDuration: 6,
        videoSyncOffsetSeconds: 5,
      }),
    ).toBe(false)

    expect(
      isVideoPreviewOutOfRange({
        selectedSecond: 11.01,
        videoDuration: 6,
        videoSyncOffsetSeconds: 5,
      }),
    ).toBe(true)
  })
})
