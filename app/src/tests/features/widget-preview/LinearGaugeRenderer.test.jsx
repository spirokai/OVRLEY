import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'
import { OverlayLinearGaugeWidget } from '@/features/widget-preview/components/LinearGaugeRenderer'

function makeWidget(overrides = {}) {
  return {
    id: 'linear-gauge-1',
    type: 'speed',
    category: 'values',
    data: {
      value: 'speed',
      display_type: 'linear',
      width: 200,
      height: 40,
      orientation: 'horizontal',
      track_corner_radius: 4,
      track_border_thickness: 2,
      track_border_color: '#ffffff',
      track_empty_color: '#222222',
      track_empty_opacity: 0.5,
      track_filled_color: '#40e0d0',
      track_filled_opacity: 1,
      track_fill_flat: false,
      show_min_max_labels: false,
      min_max_label_font: 'Arial.ttf',
      min_max_label_font_size: 12,
      min_max_label_color: '#ffffff',
      ...overrides,
    },
  }
}

const activity = {
  sample_elapsed_seconds: [0, 1],
  speed: [0, 100],
}

describe('OverlayLinearGaugeWidget', () => {
  test('applies global scale to rendered SVG size while keeping widget viewBox geometry', () => {
    render(<OverlayLinearGaugeWidget widget={makeWidget()} activity={activity} previewSecond={0} globalScale={2} />)

    const svg = screen.getByTestId('linear-gauge-preview')
    expect(svg).toHaveAttribute('width', '400')
    expect(svg).toHaveAttribute('height', '80')
    expect(svg).toHaveAttribute('viewBox', '0 0 200 40')
  })

  test('uses interpolated metric value at previewSecond', () => {
    render(<OverlayLinearGaugeWidget widget={makeWidget()} activity={activity} previewSecond={0.5} globalScale={1} />)

    const rects = screen.getByTestId('linear-gauge-preview').querySelectorAll('rect')
    // rects: [0]=mask-white, [1]=mask-black, [2]=border, [3]=empty-track, [4]=filled-track
    expect(rects[4]).toHaveAttribute('x', '2')
    expect(rects[4]).toHaveAttribute('y', '2')
    expect(rects[4]).toHaveAttribute('width', '98')
    expect(rects[4]).toHaveAttribute('height', '36')
  })

  test('renders a flat advancing fill end when enabled', () => {
    render(<OverlayLinearGaugeWidget widget={makeWidget({ track_fill_flat: true })} activity={activity} previewSecond={0.5} globalScale={1} />)

    const fill = screen.getByTestId('linear-gauge-preview').querySelector('rect[clip-path]')
    expect(fill).toHaveAttribute('fill', '#40e0d0')
    expect(fill).toHaveAttribute('width', '196')
  })

  test('uses configured label font for min/max labels', () => {
    render(
      <OverlayLinearGaugeWidget
        widget={makeWidget({ show_min_max_labels: true, min_max_label_font: 'Teko.ttf' })}
        activity={activity}
        previewSecond={0}
        globalScale={1}
      />,
    )

    const label = screen.getByText('0')
    expect(label).toHaveAttribute('font-family', '"Teko", "Arial Narrow", sans-serif')
  })
})
