import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { createPreviewSession } from '@/features/wasm-widgets/utils/previewSession'
import { previewFont, rasterPreviewPng } from '@/api/backend'
import { isWasmWidget } from '@/lib/widget/wasm-widget'

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
    _ovrley_preview_font: vi.fn(() => 0),
    _ovrley_preview_widget_create: vi.fn(() => 2),
    _ovrley_preview_widget_update: vi.fn(() => 0),
    _ovrley_preview_widget_destroy: vi.fn(),
    _ovrley_preview_render: vi.fn(() => 0),
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

test('G-force prepares its label font independently and reuses its handle through playback before disposal', async () => {
  previewFont.mockImplementation((font) => (font === 'Label.ttf' ? Promise.resolve([1, 2, 3]) : new Promise(() => {})))
  const { module, session, snapshot, error } = setup()
  const widget = { id: 'g', type: 'g_force', data: { value: 'g_force', display_type: 'g_force', font: 'Unused.ttf', label_font: 'Label.ttf' } }
  expect(isWasmWidget(widget)).toBe(true)
  const next = { ...snapshot, widgets: [widget] }
  session.setSnapshot(next)
  vi.advanceTimersToNextFrame()
  expect(module._ovrley_preview_widget_create).not.toHaveBeenCalled()
  await Promise.resolve()
  vi.advanceTimersToNextFrame()
  expect(previewFont).toHaveBeenCalledWith('Label.ttf', 700, false)
  expect(previewFont).not.toHaveBeenCalledWith('Unused.ttf', 700, false)
  expect(module._ovrley_preview_font).toHaveBeenCalledOnce()
  expect(module._ovrley_preview_widget_create).toHaveBeenCalledOnce()
  for (const second of [1.25, 3.5, 0.5]) {
    session.setSnapshot({ ...next, second })
    vi.advanceTimersToNextFrame()
  }
  expect(module._ovrley_preview_render.mock.calls.map((call) => call[1])).toEqual([0, 0, 1.25, 3.5, 0.5])
  expect(module._ovrley_preview_widget_update).not.toHaveBeenCalled()
  session.setSnapshot({ ...next, widgets: [] })
  vi.advanceTimersToNextFrame()
  expect(module._ovrley_preview_widget_destroy).toHaveBeenCalledWith(1, 2)
  expect(module._ovrley_preview_destroy).not.toHaveBeenCalled()
  expect(error).toHaveBeenLastCalledWith(null)
  session.dispose()
  expect(module._ovrley_preview_destroy).toHaveBeenCalledOnce()
})
