import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

describe('backend extractVideoTelemetry', () => {
  beforeEach(() => {
    vi.resetModules()
    window.__TAURI_INTERNALS__ = {}
  })

  afterEach(() => {
    delete window.__TAURI_INTERNALS__
    vi.restoreAllMocks()
    vi.resetModules()
  })

  test('invokes backend_extract_video_telemetry with filePath', async () => {
    const invoke = vi.fn().mockResolvedValue(JSON.stringify({ metadata: {}, file_format: 'mp4-telemetry' }))
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }))

    const backend = await import('@/api/backend')

    const result = await backend.extractVideoTelemetry('/path/to/video.mp4')

    expect(invoke).toHaveBeenCalledWith('backend_extract_video_telemetry', { filePath: '/path/to/video.mp4' })
    expect(result).toEqual({ metadata: {}, file_format: 'mp4-telemetry' })
  })

  test('returns null when backend returns null', async () => {
    const invoke = vi.fn().mockResolvedValue(JSON.stringify(null))
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }))

    const backend = await import('@/api/backend')

    const result = await backend.extractVideoTelemetry('/path/to/video.mp4')

    expect(result).toBeNull()
  })
})
