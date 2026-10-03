import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import useOverlayPreviewModels from '@/features/overlay-editor/hooks/useOverlayPreviewModels'
import { buildWidgetRenderGeometryModels } from '@/features/overlay-editor/utils/widgetRenderGeometry'
import { prepareFont } from '@/lib/font-resources'
import { createMetricValueDefaults } from '@/features/widget-editor/utils/widgetUtils'

vi.mock('@/api/backend', () => ({
  listAvailableFonts: async () => ({
    recommendedFonts: [
      {
        id: 'Selection test.ttf',
        name: 'Selection test',
        faces: [
          {
            style: 'normal',
            weight: 400,
            axes: [{ tag: 'wght', min: 100, default: 400, max: 900, hidden: false }],
            file: null,
            local_name: 'Selection test',
          },
          {
            style: 'italic',
            weight: 400,
            axes: [{ tag: 'wght', min: 100, default: 400, max: 900, hidden: false }],
            file: null,
            local_name: 'Selection test italic',
          },
        ],
      },
    ],
    systemFonts: [],
  }),
}))

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

test('prepares fonts before the first measured preview and updates bounds on style changes without loading', async () => {
  let available = false
  let completeLoad
  const ready = new Promise((resolve) => {
    completeLoad = resolve
  })
  Object.defineProperty(document, 'fonts', {
    configurable: true,
    value: { add: vi.fn() },
  })
  const load = vi.fn(async function () {
    await ready
    return this
  })
  vi.stubGlobal(
    'FontFace',
    class {
      constructor(family) {
        this.family = family
      }
      load = load
    },
  )
  const preparation = prepareFont('Selection test.ttf')
  expect(document.fonts.add).not.toHaveBeenCalled()
  available = true
  completeLoad()
  await preparation
  expect(load).toHaveBeenCalledTimes(2)
  const context = {
    font: '',
    measureText(text) {
      const width = text === 'Weighted label' ? (available ? (this.font.startsWith('900 ') ? 140 : 110) : 80) : text.length * 10
      const italic = this.font.startsWith('italic ')
      return {
        width,
        actualBoundingBoxLeft: italic ? 3 : 0,
        actualBoundingBoxRight: width + (italic ? 8 : 0),
        actualBoundingBoxAscent: 30,
        actualBoundingBoxDescent: 5,
      }
    },
  }
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(context)
  const widget = {
    id: 'label-selection',
    category: 'labels',
    type: 'label',
    data: {
      text: 'Weighted label',
      font: 'Selection test.ttf',
      font_size: 40,
      font_weight: 537,
      italic: false,
      letter_spacing: 0,
      x: 20,
      y: 20,
      rotation: 0,
    },
  }
  const { result, rerender } = renderHook(
    ({ weight, italic, previewSecond = 0 }) => {
      const widgets = [
        { ...widget, data: { ...widget.data, font_weight: weight, italic } },
        {
          id: 'speed',
          type: 'speed',
          category: 'values',
          data: { ...createMetricValueDefaults('speed'), font: 'Selection test.ttf', show_icon: false, show_units: false },
        },
      ]
      const activity = { trim_end_seconds: 10, sample_elapsed_seconds: [0, 10], speed: [1, 100] }
      const models = useOverlayPreviewModels({ renderedWidgets: widgets, activity, previewSecond, exportStartSecond: 0, globalScale: 1 })
      return buildWidgetRenderGeometryModels({ widgets, ...models, globalScale: 1 })
    },
    { initialProps: { weight: 537, italic: false } },
  )
  expect(result.current['label-selection'].renderGeometry.width).toBe(110)
  rerender({ weight: 900, italic: false })
  await waitFor(() => expect(result.current['label-selection'].renderGeometry.width).toBe(140))
  rerender({ weight: 537, italic: true })
  await waitFor(() => expect(result.current['label-selection'].renderGeometry.width).toBe(121))
  const initialSpeedWidth = result.current.speed.renderGeometry.width
  rerender({ weight: 537, italic: true, previewSecond: 10 })
  expect(result.current.speed.renderGeometry.width).toBeGreaterThan(initialSpeedWidth)
  expect(load).toHaveBeenCalledTimes(2)
})
