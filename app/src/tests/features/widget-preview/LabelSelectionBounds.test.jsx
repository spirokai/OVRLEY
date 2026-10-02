import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import useOverlayPreviewModels from '@/features/overlay-editor/hooks/useOverlayPreviewModels'
import { buildWidgetRenderGeometryModels } from '@/features/overlay-editor/utils/widgetRenderGeometry'
import { setFontCatalog } from '@/lib/fonts'

afterEach(() => vi.restoreAllMocks())

test('selection bounds refresh after font loading and include italic overhangs alongside weight changes', async () => {
  setFontCatalog({
    recommendedFonts: [
      {
        id: 'Selection test.ttf',
        name: 'Selection test',
        faces: [
          { style: 'normal', weight: 400, axes: [{ tag: 'wght', min: 100, default: 400, max: 900, hidden: false }] },
          { style: 'italic', weight: 400, axes: [{ tag: 'wght', min: 100, default: 400, max: 900, hidden: false }] },
        ],
      },
    ],
    systemFonts: [],
  })
  let available = false
  let completeLoad
  const ready = new Promise((resolve) => {
    completeLoad = resolve
  })
  Object.defineProperty(document, 'fonts', {
    configurable: true,
    value: { ready: Promise.resolve(), load: vi.fn(() => ready) },
  })
  const context = {
    font: '',
    measureText() {
      const width = available ? (this.font.startsWith('900 ') ? 140 : 110) : 80
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
    ({ weight, italic }) => {
      const widgets = [{ ...widget, data: { ...widget.data, font_weight: weight, italic } }]
      const models = useOverlayPreviewModels({ renderedWidgets: widgets, globalScale: 1 })
      return buildWidgetRenderGeometryModels({ widgets, ...models, globalScale: 1 })
    },
    { initialProps: { weight: 537, italic: false } },
  )
  expect(result.current['label-selection'].renderGeometry.width).toBe(80)
  await act(async () => {
    available = true
    completeLoad()
  })
  await waitFor(() => expect(result.current['label-selection'].renderGeometry.width).toBe(110))
  expect(document.fonts.load).toHaveBeenCalledWith('537 40px "OVRLEY Selection test.ttf"', '0123456789WBMPRK/H')
  rerender({ weight: 900, italic: false })
  await waitFor(() => expect(result.current['label-selection'].renderGeometry.width).toBe(140))
  expect(document.fonts.load).toHaveBeenCalledWith('900 40px "OVRLEY Selection test.ttf"', '0123456789WBMPRK/H')
  rerender({ weight: 537, italic: true })
  await waitFor(() => expect(result.current['label-selection'].renderGeometry.width).toBe(121))
  expect(document.fonts.load).toHaveBeenCalledWith('italic 537 40px "OVRLEY Selection test.ttf"', '0123456789WBMPRK/H')
})
