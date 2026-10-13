import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { createPreviewSession } from '@/features/wasm-widgets/utils/previewSession'
import { previewFont, rasterPreviewPng } from '@/api/backend'

vi.mock('@/api/backend', () => ({ previewFont: vi.fn(), previewActivity: vi.fn(), rasterPreviewPng: vi.fn() }))

beforeEach(() => {
  vi.resetAllMocks()
  vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] })
  previewFont.mockReturnValue(new Promise(() => {}))
})
afterEach(() => vi.useRealTimers())

function setup() {
  const module = {
    HEAPU8: new Uint8Array(1024),
    _ovrley_alloc: vi.fn(() => 64),
    _ovrley_dealloc: vi.fn(),
    _ovrley_preview_create: vi.fn(() => 1),
    _ovrley_preview_destroy: vi.fn(),
    _ovrley_preview_activity: vi.fn(() => 0),
    _ovrley_preview_image: vi.fn(() => 0),
    _ovrley_preview_widget_create: vi.fn(() => 2),
    _ovrley_preview_widget_update: vi.fn(() => 0),
    _ovrley_preview_widget_destroy: vi.fn(),
    _ovrley_preview_render: vi.fn(),
  }
  const error = vi.fn()
  const session = createPreviewSession(module, error)
  const snapshot = {
    widgets: [{ id: 'r', type: 'raster', data: { resourceId: 'image', path: '/image.png' } }],
    scene: {},
    defaultFont: 'Inter.ttf',
    activityId: null,
    second: 0,
  }
  return { module, session, snapshot, error }
}

test('raster loads once, creates without waiting for fonts, and ignores playhead-only snapshots', async () => {
  rasterPreviewPng.mockResolvedValue('AQID')
  const { module, session, snapshot } = setup()
  session.setSnapshot(snapshot)
  await Promise.resolve()
  vi.advanceTimersToNextFrame()
  expect(module._ovrley_preview_image).toHaveBeenCalledTimes(1)
  expect(module._ovrley_preview_widget_create).toHaveBeenCalledTimes(1)
  for (const second of [1, 20, 2]) session.setSnapshot({ ...snapshot, second })
  expect(vi.getTimerCount()).toBe(0)
  expect(module._ovrley_preview_render).not.toHaveBeenCalled()
  expect(module._ovrley_preview_widget_update).not.toHaveBeenCalled()
  session.setSnapshot({ ...snapshot, widgets: [] })
  vi.advanceTimersToNextFrame()
  expect(module._ovrley_preview_widget_destroy).toHaveBeenCalledWith(1, 2)
  session.setSnapshot(snapshot)
  vi.advanceTimersToNextFrame()
  expect(rasterPreviewPng).toHaveBeenCalledTimes(1)
  session.dispose()
  expect(module._ovrley_preview_destroy).toHaveBeenCalledOnce()
})
