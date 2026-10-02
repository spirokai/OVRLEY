import { render } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { OverlayTextWidget } from '@/features/widget-preview/widgets/text/TextPreview'

vi.mock('@/features/widget-preview/shared/useFontMetrics', () => ({
  useFontMetrics: () => 0,
}))

vi.mock('@/features/widget-preview/shared/textMeasurement', async () => {
  const actual = await vi.importActual('@/features/widget-preview/shared/textMeasurement')
  return {
    ...actual,
    getPreviewFontFamily: (fontFamily) => fontFamily || 'Arial',
    getWidgetOpacity: () => 1,
  }
})

describe('OverlayTextWidget', () => {
  test('preserves the original label text casing in the SVG text nodes', () => {
    const { container } = render(
      <OverlayTextWidget
        widget={{
          id: 'label-1',
          type: 'label',
          category: 'labels',
          data: { text: 'MiXeD Case', font_size: 32, font_weight: 537, italic: true, letter_spacing: 0, color: '#ffffff' },
        }}
        globalOpacity={1}
        sceneStyle={{ shadow_color: '#000000', shadow_strength: 2, shadow_distance: 3, border_color: '#ff0000', border_thickness: 1 }}
        textPreviewModel={{
          fontWeight: 537,
          fontStyle: 'italic',
          text: 'MiXeD Case',
          baseline: 24,
          measurement: { width: 120 },
          lineHeight: 32,
          visualBounds: { width: 120, height: 32, offsetX: 0, offsetY: 0 },
        }}
      />,
    )

    const textNodes = container.querySelectorAll('text')

    expect(textNodes).toHaveLength(2)
    expect(Array.from(textNodes).every((node) => node.style.textTransform === 'none')).toBe(true)
    expect(Array.from(textNodes).some((node) => node.textContent === 'MiXeD Case')).toBe(true)
    expect(Array.from(textNodes).every((node) => node.getAttribute('font-weight') === '537')).toBe(true)
    expect(Array.from(textNodes).every((node) => node.getAttribute('font-style') === 'italic')).toBe(true)
    expect(container.querySelector('text[stroke="#ff0000"]')).toHaveAttribute('font-weight', '537')
  })
})
