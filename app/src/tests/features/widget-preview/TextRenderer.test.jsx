import { render } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { OverlayTextWidget } from '@/features/widget-preview/components/TextRenderer'

vi.mock('@/features/widget-preview/hooks/useFontMetricsVersion', () => ({
  useFontMetricsVersion: () => 0,
}))

vi.mock('@/features/widget-preview/utils/textMeasurement', async () => {
  const actual = await vi.importActual('@/features/widget-preview/utils/textMeasurement')
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
        widget={{ id: 'label-1', type: 'label', category: 'labels', data: { text: 'MiXeD Case', font_size: 32, color: '#ffffff' } }}
        globalOpacity={1}
        sceneStyle={{}}
        textPreviewModel={{
          text: 'MiXeD Case',
          baseline: 24,
          measurement: { width: 120 },
          lineHeight: 32,
          visualBounds: { width: 120, height: 32, offsetX: 0, offsetY: 0 },
        }}
      />,
    )

    const textNodes = container.querySelectorAll('text')

    expect(textNodes.length).toBeGreaterThan(0)
    expect(Array.from(textNodes).every((node) => node.style.textTransform === 'none')).toBe(true)
    expect(Array.from(textNodes).some((node) => node.textContent === 'MiXeD Case')).toBe(true)
  })
})
