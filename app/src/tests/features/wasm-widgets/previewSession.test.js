import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { createPreviewSession } from '@/features/wasm-widgets/utils/previewSession'
import { previewActivity, previewFont, rasterPreviewPng } from '@/api/backend'

vi.mock('@/api/backend', () => ({ previewFont: vi.fn(), previewActivity: vi.fn(), rasterPreviewPng: vi.fn() }))

beforeEach(() => {
  vi.resetAllMocks()
  vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] })
  previewFont.mockReturnValue(new Promise(() => {}))
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

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

test('elevation playback reuses its raster allocation and skips placement work when only pixels change', async () => {
  previewFont.mockResolvedValue([1, 2, 3])
  const { module, session, snapshot, error } = setup()
  const geometry = new TextEncoder().encode(
    JSON.stringify({ left: 0, top: 0, width: 2, height: 2, content_width: 2, content: { left: 0, top: 0, width: 2, height: 2 } }),
  )
  module.HEAPU8.set(geometry, 512)
  module._ovrley_preview_revision = vi.fn(() => module._ovrley_preview_render.mock.calls.length)
  module._ovrley_preview_geometry_ptr = vi.fn(() => 512)
  module._ovrley_preview_geometry_len = vi.fn(() => geometry.length)
  module._ovrley_preview_pixels = vi.fn(() => 256)
  const allocateImage = vi.fn()
  vi.stubGlobal(
    'ImageData',
    class {
      constructor(width, height) {
        allocateImage(width, height)
        this.width = width
        this.height = height
        this.data = new Uint8ClampedArray(width * height * 4)
      }
    },
  )
  const target = document.createElement('div')
  const canvas = document.createElement('canvas')
  target.append(canvas)
  const putImageData = vi.fn()
  canvas.getContext = vi.fn(() => ({ putImageData }))
  const placement = vi.fn()
  target.addEventListener('wasm-widget-render', placement)
  const next = {
    ...snapshot,
    widgets: [
      { id: 'e', type: 'elevation', data: { show_elevation_metric: true, show_elevation_imperial: false, point_label: { font: 'Label.ttf' } } },
    ],
  }
  session.attach('e', canvas)
  session.setSnapshot(next)
  await Promise.resolve()
  vi.advanceTimersToNextFrame()
  for (const second of [0.125, 1.5, 0.5]) {
    session.setSnapshot({ ...next, second })
    vi.advanceTimersToNextFrame()
  }
  expect(error).toHaveBeenLastCalledWith(null)
  expect(allocateImage).toHaveBeenCalledOnce()
  expect(putImageData).toHaveBeenCalledTimes(4)
  expect(placement).toHaveBeenCalledOnce()
  expect(module._ovrley_preview_widget_create).toHaveBeenCalledOnce()
  expect(module._ovrley_preview_widget_update).not.toHaveBeenCalled()
  expect(previewFont).toHaveBeenCalledTimes(2)
  session.dispose()
})

test.each(['g_force', 'heading', 'elevation'])(
  '%s prepares its label font independently and reuses its handle through playback before disposal',
  async (type) => {
    previewFont.mockImplementation(({ font }) => (font === 'Label.ttf' ? Promise.resolve([1, 2, 3]) : new Promise(() => {})))
    const { module, session, snapshot, error } = setup()
    const widget = {
      id: 'g',
      type,
      data: {
        value: type,
        display_type: type === 'heading' ? 'heading_tape' : type,
        font: 'Unused.ttf',
        label_font: 'Label.ttf',
        ...(type === 'elevation' ? { point_label: { font: 'Label.ttf' }, show_elevation_metric: true, show_elevation_imperial: false } : {}),
      },
    }
    const next = { ...snapshot, widgets: [widget] }
    session.setSnapshot(next)
    vi.advanceTimersToNextFrame()
    expect(module._ovrley_preview_widget_create).not.toHaveBeenCalled()
    await Promise.resolve()
    vi.advanceTimersToNextFrame()
    expect(previewFont).toHaveBeenCalledWith({ font: 'Label.ttf' })
    expect(previewFont).not.toHaveBeenCalledWith({ font: 'Unused.ttf' })
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
  },
)

test.each(['linear', 'arc', 'corner'])(
  '%s transports its required gauge fonts and keeps the handle through fractional playback',
  async (displayType) => {
    previewFont.mockResolvedValue([1, 2, 3])
    const { module, session, snapshot, error } = setup()
    const widget = {
      id: 'gauge',
      type: 'speed',
      data: { value: 'speed', display_type: displayType, font: 'Value.ttf', min_max_label_font: 'Range.ttf', show_min_max_labels: true },
    }
    const next = { ...snapshot, widgets: [widget] }
    session.setSnapshot(next)
    await Promise.resolve()
    vi.advanceTimersToNextFrame()
    expect(previewFont).toHaveBeenCalledWith({ font: 'Range.ttf' })
    if (displayType === 'linear') expect(previewFont).not.toHaveBeenCalledWith({ font: 'Value.ttf' })
    else expect(previewFont).toHaveBeenCalledWith({ font: 'Value.ttf' })
    expect(module._ovrley_preview_widget_create).toHaveBeenCalledOnce()
    for (const second of [1.125, 0.501, 3.75]) {
      session.setSnapshot({ ...next, second })
      vi.advanceTimersToNextFrame()
    }
    expect(module._ovrley_preview_render.mock.calls.map((call) => call[1])).toEqual([0, 1.125, 0.501, 3.75])
    expect(module._ovrley_preview_widget_update).not.toHaveBeenCalled()
    session.setSnapshot({ ...next, widgets: [] })
    vi.advanceTimersToNextFrame()
    expect(module._ovrley_preview_widget_destroy).toHaveBeenCalledWith(1, 2)
    expect(error).toHaveBeenLastCalledWith(null)
    session.dispose()
  },
)

test('lap timers load both fonts and activity independently, retain resources, and surface label-font errors', async () => {
  let finishLabel
  let finishLateFont
  const failedFont = new Error('Unable to load label font')
  previewFont.mockImplementation(({ font }) => {
    if (font === 'Value.ttf') return Promise.resolve([1, 2, 3])
    if (font === 'Label.ttf') return new Promise((resolve) => (finishLabel = resolve))
    if (font === 'Failed.ttf') return Promise.reject(failedFont)
    if (font === 'Late.ttf') return new Promise((resolve) => (finishLateFont = resolve))
    return new Promise(() => {})
  })
  previewActivity.mockResolvedValue([4, 5, 6])
  const { module, session, snapshot, error } = setup()
  const widget = { id: 'lap', type: 'lap_timer', data: { value: 'lap_timer', display_type: 'lap_timer', font: 'Value.ttf', label_font: 'Label.ttf' } }
  const next = { ...snapshot, widgets: [widget], activityId: 'activity' }
  session.setSnapshot(next)
  expect(previewFont).toHaveBeenCalledWith({ font: 'Value.ttf' })
  expect(previewFont).toHaveBeenCalledWith({ font: 'Label.ttf' })
  expect(previewActivity).toHaveBeenCalledWith('activity')
  await Promise.resolve()
  vi.advanceTimersToNextFrame()
  expect(module._ovrley_preview_activity).toHaveBeenCalledTimes(2) // Clear old samples, then transport new bytes.
  expect(module._ovrley_preview_widget_create).not.toHaveBeenCalled()
  finishLabel([7, 8, 9])
  await Promise.resolve()
  vi.advanceTimersToNextFrame()
  expect(module._ovrley_preview_widget_create).toHaveBeenCalledOnce()
  for (const second of [1.125, 4, 0.5]) {
    session.setSnapshot({ ...next, second })
    vi.advanceTimersToNextFrame()
  }
  expect(module._ovrley_preview_render.mock.lastCall[1]).toBe(0.5)
  expect(module._ovrley_preview_widget_update).not.toHaveBeenCalled()
  session.setSnapshot({ ...next, widgets: [] })
  vi.advanceTimersToNextFrame()
  expect(module._ovrley_preview_widget_destroy).toHaveBeenCalledWith(1, 2)
  expect(module._ovrley_preview_destroy).not.toHaveBeenCalled()
  session.setSnapshot(next)
  vi.advanceTimersToNextFrame()
  expect(previewFont).toHaveBeenCalledTimes(3) // Includes independently prewarmed default font.
  const failed = { ...next, widgets: [{ ...widget, data: { ...widget.data, label_font: 'Failed.ttf' } }] }
  session.setSnapshot(failed)
  await Promise.resolve()
  await Promise.resolve()
  vi.advanceTimersToNextFrame()
  expect(error).toHaveBeenLastCalledWith(failedFont)
  session.setSnapshot({ ...failed, second: 2 })
  vi.advanceTimersToNextFrame()
  expect(error).toHaveBeenLastCalledWith(failedFont)
  session.setSnapshot({ ...next, widgets: [{ ...widget, data: { ...widget.data, label_font: 'Late.ttf' } }] })
  session.dispose()
  finishLateFont([10, 11, 12])
  await Promise.resolve()
  expect(module._ovrley_preview_font).toHaveBeenCalledTimes(2)
  expect(module._ovrley_preview_destroy).toHaveBeenCalledOnce()
  expect(vi.getTimerCount()).toBe(0)
})
