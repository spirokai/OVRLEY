/**
 * Regression tests for preview-source resolution helpers.
 *
 * The preview hook should resolve one clear source of truth for the `<video>`
 * element.
 */

import { describe, expect, test, vi } from 'vite-plus/test'
import { resolveVideoPreviewSource } from '@/features/video-preview/utils/videoPreviewSource'

describe('resolveVideoPreviewSource', () => {
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
})
