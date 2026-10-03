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

  test('preserves a tagged backend rejection for an unselected raster in preview and export', async () => {
    const backendMessage = 'Invalid configuration: Raster widget-1 has no selected image. [raster_error:no_image:widget-1]'
    const invoke = vi.fn().mockRejectedValue(backendMessage)
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }))
    const backend = await import('@/api/backend')
    const config = { rasters: [{ id: 'widget-1', path: null }] }

    await expect(backend.renderVideo(config, {}, { outputPath: 'C:\\renders\\overlay.mov' })).rejects.toThrow(backendMessage)
    await expect(backend.renderPreviewFrame(config, {}, 0)).rejects.toThrow(backendMessage)
    expect(invoke).toHaveBeenCalledTimes(2)
  })

  test('submits a selected raster resource for preview and export', async () => {
    const invoke = vi.fn().mockResolvedValue('{}')
    vi.doMock('@tauri-apps/api/core', () => ({ invoke }))
    const backend = await import('@/api/backend')
    const config = { rasters: [{ id: 'widget-1', path: 'C:\\image.png', resourceId: 'snapshot-1' }] }

    await backend.renderPreviewFrame(config, {}, 0)
    await backend.renderVideo(config, {}, { outputPath: 'C:\\renders\\overlay.mov' })
    expect(invoke).toHaveBeenCalledTimes(2)
    expect(JSON.parse(invoke.mock.calls[0][1].configJson).rasters[0].resourceId).toBe('snapshot-1')
    expect(JSON.parse(invoke.mock.calls[1][1].configJson).rasters[0].resourceId).toBe('snapshot-1')
    expect(invoke.mock.calls[0][1]).not.toHaveProperty('rasterResourceIds')
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
