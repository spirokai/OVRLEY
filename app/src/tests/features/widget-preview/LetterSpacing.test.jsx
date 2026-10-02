import { render } from '@testing-library/react'
import { beforeAll, expect, test, vi } from 'vitest'
import { OverlayTextWidget } from '@/features/widget-preview/widgets/text/TextPreview'
import { buildTextWidgetPreviewModel } from '@/features/widget-preview/widgets/text/model'
import { resolveWidgetRenderGeometry } from '@/features/overlay-editor/utils/widgetRenderGeometry'

vi.mock('@/lib/font-resources', () => ({
  getPreparedFont: () =>
    ({
      recommendedFonts: [
        {
          id: 'Spacing',
          name: 'Spacing',
          faces: [
            { style: 'normal', weight: 400, axes: [] },
            { style: 'italic', weight: 537, axes: [] },
          ],
        },
      ],
      systemFonts: [],
    }).recommendedFonts[0],
}))

beforeAll(() => {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    font: '',
    measureText(text) {
      const clusters = [...new Intl.Segmenter('und', { granularity: 'grapheme' }).segment(text)]
      const width = text === 'AV' ? 17 : clusters.reduce((sum, { segment }) => sum + (segment === ' ' ? 5 : 10), 0)
      const ink = text.trim().length > 0
      return {
        width,
        actualBoundingBoxLeft: ink ? 3 : 0,
        actualBoundingBoxRight: ink ? width + 2 : 0,
        actualBoundingBoxAscent: ink ? 8 : 0,
        actualBoundingBoxDescent: ink ? 2 : 0,
      }
    },
  })
})

const label = (text, letter_spacing) => ({
  id: 'spacing',
  category: 'labels',
  type: 'label',
  data: { text, letter_spacing, font: 'Spacing', font_size: 20, font_weight: 537, italic: true, color: '#ffffff', opacity: 1, x: 50, y: 20 },
})

test('spacing preserves graphemes and whitespace with no trailing advance in every SVG paint layer', () => {
  const widget = label('e\u0301 😀é', 12.5)
  const model = buildTextWidgetPreviewModel({ widget })
  expect(model.measurement.width).toBe(42.5)
  expect(model.measurement.runs).toEqual([
    { text: 'e\u0301', x: 0 },
    { text: ' ', x: 12.5 },
    { text: '😀', x: 20 },
    { text: 'é', x: 32.5 },
  ])
  const { container } = render(
    <OverlayTextWidget
      widget={widget}
      globalOpacity={1}
      sceneStyle={{
        shadow_color: '#000000',
        shadow_strength: 2,
        shadow_distance: 3,
        border_color: '#ff0000',
        border_thickness: 1,
      }}
      textPreviewModel={model}
    />,
  )
  const layers = container.querySelectorAll('text')
  expect(layers).toHaveLength(2)
  for (const layer of layers) {
    expect(layer.textContent).toBe(widget.data.text)
    expect(layer).toHaveAttribute('font-weight', '537')
    expect(layer).toHaveAttribute('font-style', 'italic')
    expect([...layer.querySelectorAll('tspan')].map((node) => node.getAttribute('x'))).toEqual(['3', '15.5', '23', '35.5'])
  }
  expect(container.querySelector('text[stroke="#ff0000"]')).toBeInTheDocument()
})

test('negative spacing bounds include overlapping ink even when the advance becomes negative, and scale with the scene', () => {
  const widget = label('AB', -125)
  const model = buildTextWidgetPreviewModel({ widget })
  expect(model.measurement.width).toBe(-5)
  expect(model.visualBounds).toMatchObject({ minX: -18, maxX: 12, width: 30, height: 10 })
  expect(model.baseline).toBeCloseTo(12.2)
  const geometry = resolveWidgetRenderGeometry(widget, model.visualBounds, 2)
  expect(geometry).toMatchObject({ left: 14, width: 30, transform: 'scale(2)' })
})

test('zero spacing retains whole-text layout; empty and single graphemes have no gaps', () => {
  expect(buildTextWidgetPreviewModel({ widget: label('AV', 0) }).measurement.width).toBe(17)
  for (const spacing of [-2.5, 0, 2.5]) {
    expect(buildTextWidgetPreviewModel({ widget: label('', spacing) }).visualBounds.width).toBe(0)
    const whitespace = buildTextWidgetPreviewModel({ widget: label(' ', spacing) })
    expect(whitespace.measurement.width).toBe(5)
    expect(whitespace.visualBounds.width).toBe(0)
    for (const text of ['A', 'é', 'e\u0301', '😀']) {
      expect(buildTextWidgetPreviewModel({ widget: label(text, spacing) }).measurement.width).toBe(10)
    }
  }
})

test('percentage spacing follows font size without changing the saved percentage', () => {
  const widget = label('AB', 10)
  const small = buildTextWidgetPreviewModel({ widget })
  const large = buildTextWidgetPreviewModel({ widget: { ...widget, data: { ...widget.data, font_size: 40 } } })
  expect(small.measurement.runs[1].x).toBe(12)
  expect(large.measurement.runs[1].x).toBe(14)
  expect(widget.data.letter_spacing).toBe(10)
})
