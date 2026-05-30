import { describe, expect, test } from 'vitest'
import { render } from '@testing-library/react'

import { OverlayHeadingWidget } from '@/features/widget-preview'

function makeHeadingWidget(overrides = {}) {
  return {
    id: 'test-heading-1',
    type: 'heading',
    category: 'plots',
    data: {
      x: 100,
      y: 200,
      width: 400,
      height: 80,
      pixels_per_degree: 5,
      major_tick_interval: 15,
      minor_ticks_per_major: 3,
      show_major_ticks: true,
      show_minor_ticks: true,
      major_tick_length_pct: 40,
      minor_tick_length_pct: 20,
      major_tick_thickness: 2,
      minor_tick_thickness: 1,
      tick_color: '#ffffff',
      cardinal_tick_color: '#ff0000',
      tick_alignment: 'below',
      show_minor_labels: true,
      show_major_labels: true,
      label_color: '#cccccc',
      cardinal_label_color: '#ff0000',
      label_font_size: 12,
      show_indicator: true,
      indicator_style: 'chevron',
      indicator_placement: 'top',
      indicator_color: '#ff0000',
      indicator_size: 10,
      ...overrides,
    },
  }
}

function makeActivity(headingSeries = [90, 91, 92]) {
  return {
    sample_elapsed_seconds: headingSeries.map((_, i) => i),
    heading: headingSeries,
  }
}

describe('OverlayHeadingWidget', () => {
  test('renders an SVG element', () => {
    const widget = makeHeadingWidget()
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} />)
    const svg = container.querySelector('svg')
    expect(svg).toBeTruthy()
  })

  test('renders with correct dimensions from widget config', () => {
    const widget = makeHeadingWidget({ width: 500, height: 100 })
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} />)
    const svg = container.querySelector('svg')
    expect(svg).toHaveAttribute('width', '500')
    expect(svg).toHaveAttribute('height', '100')
  })

  test('renders a tape pattern element', () => {
    const widget = makeHeadingWidget()
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} />)
    const pattern = container.querySelector('pattern')
    expect(pattern).toBeTruthy()
  })

  test('renders the tape rect filled with the pattern', () => {
    const widget = makeHeadingWidget()
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} />)
    const rects = container.querySelectorAll('rect')
    expect(rects.length).toBeGreaterThan(0)
  })

  test('renders chevron indicator when indicator_style is chevron', () => {
    const widget = makeHeadingWidget({ indicator_style: 'chevron', indicator_placement: 'top' })
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} />)
    const polygons = container.querySelectorAll('polygon')
    expect(polygons.length).toBeGreaterThan(0)
  })

  test('renders highlight bar indicator when indicator_style is highlight_bar', () => {
    const widget = makeHeadingWidget({ indicator_style: 'highlight_bar', indicator_placement: 'both' })
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} />)
    const elements = container.querySelectorAll('rect, polygon')
    expect(elements.length).toBeGreaterThan(0)
    expect(container.querySelectorAll('polygon')).toHaveLength(0)
  })

  test('hides indicator when show_indicator is false', () => {
    const widget = makeHeadingWidget({ show_indicator: false })
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} />)
    // Only the tape pattern elements should exist, no extra indicator polygons
    const polygons = container.querySelectorAll('polygon')
    expect(polygons.length).toBe(0)
  })

  test('renders fallback display when no activity data', () => {
    const widget = makeHeadingWidget()
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={null} previewSecond={0} globalOpacity={1} />)
    const svg = container.querySelector('svg')
    expect(svg).toBeTruthy()
  })

  test('applies global opacity', () => {
    const widget = makeHeadingWidget()
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={0.5} />)
    const svg = container.querySelector('svg')
    expect(svg).toBeTruthy()
    expect(svg.style.opacity).toBe('0.5')
  })

  test('renders major and minor ticks with separate thickness values', () => {
    const widget = makeHeadingWidget({ major_tick_thickness: 4, minor_tick_thickness: 1 })
    const { container } = render(<OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} />)
    const lines = Array.from(container.querySelectorAll('line'))
    const majorTick = lines.find((line) => line.getAttribute('stroke-width') === '4')
    const minorTick = lines.find((line) => line.getAttribute('stroke-width') === '1')

    expect(majorTick).toBeTruthy()
    expect(minorTick).toBeTruthy()
  })

  test('heading labels inherit the value font when no label font is set', () => {
    const widget = makeHeadingWidget({ label_font: undefined, label_font_family: undefined })
    const { container } = render(
      <OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} valueFont="Furore.otf" />,
    )
    const label = container.querySelector('text')

    expect(label).toHaveAttribute('font-family', '"Furore", "Arial Black", Impact, sans-serif')
  })

  test('heading label font overrides the inherited value font', () => {
    const widget = makeHeadingWidget({ label_font: 'Teko.ttf' })
    const { container } = render(
      <OverlayHeadingWidget widget={widget} activity={makeActivity()} previewSecond={0} globalOpacity={1} valueFont="Furore.otf" />,
    )
    const label = container.querySelector('text')

    expect(label).toHaveAttribute('font-family', '"Teko", "Arial Narrow", sans-serif')
  })
})
