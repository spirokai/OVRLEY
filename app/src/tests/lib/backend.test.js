import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

describe('backend Tauri error normalization', () => {
  beforeEach(() => {
    vi.resetModules()
    window.__TAURI_INTERNALS__ = {}
  })

  afterEach(() => {
    delete window.__TAURI_INTERNALS__
    vi.restoreAllMocks()
    vi.resetModules()
  })

  test('writeTemplateFile turns string bridge rejections into Error instances', async () => {
    const invoke = vi.fn().mockRejectedValue('Disk full')
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }))

    const backend = await import('@/api/backend')

    await expect(backend.writeTemplateFile('C:\\templates\\acid.json', '{}')).rejects.toThrow('Disk full')
  })

  test('getDefaultTemplateSavePath preserves object message text from bridge rejections', async () => {
    const invoke = vi.fn().mockRejectedValue({ message: 'Documents folder unavailable' })
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }))

    const backend = await import('@/api/backend')

    await expect(backend.getDefaultTemplateSavePath('acid.json')).rejects.toThrow('Documents folder unavailable')
  })
})
