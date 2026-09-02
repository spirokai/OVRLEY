import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

describe('backend Tauri error normalization', () => {
  beforeEach(() => {
    vi.resetModules()
    window.__TAURI_INTERNALS__ = {}
    vi.spyOn(console, 'error').mockImplementation(() => {})
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

  test('render rejection preserves the backend error code', async () => {
    const invoke = vi.fn().mockRejectedValue({ code: 'already_exists', message: 'Output already exists' })
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }))

    const backend = await import('@/api/backend')

    await expect(
      backend.renderVideo(
        {},
        {},
        {
          outputPath: 'C:\\renders\\overlay.mov',
          overwrite: false,
        },
      ),
    ).rejects.toMatchObject({ code: 'already_exists', message: 'Output already exists' })
  })

  test('listProjectFiles accepts canonical optional thumbnail data', async () => {
    const projects = [
      {
        name: 'Race',
        path: 'C:\\Projects\\Race.oly',
        thumbnailDataUrl: 'data:image/png;base64,dGh1bWJuYWls',
      },
    ]
    const invoke = vi.fn().mockResolvedValue(projects)
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }))
    const backend = await import('@/api/backend')

    await expect(backend.listProjectFiles('C:\\Projects')).resolves.toEqual(projects)
  })

  test('listProjectFiles rejects malformed thumbnail data', async () => {
    const invoke = vi.fn().mockResolvedValue([{ name: 'Race', path: 'C:\\Projects\\Race.oly', thumbnailDataUrl: 'file:///thumbnail.png' }])
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }))
    const backend = await import('@/api/backend')

    await expect(backend.listProjectFiles('C:\\Projects')).rejects.toThrow('Invalid project list returned by backend')
  })
})
