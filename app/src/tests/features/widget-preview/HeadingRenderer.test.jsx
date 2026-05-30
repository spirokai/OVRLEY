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
      tick_thickness: 2,
      tick_color: '#ffffff',
      cardinal_tick_color: '#ff0000',
      tick_alignment: 'below',
      show_numeric_labels: true,
      show_cardinal_labels: true,
      numeric_label_color: '#cccccc',
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
    // Highlight bar has a rect for the bar + polygons for edge markers
    const elements = container.querySelectorAll('rect, polygon')
    expect(elements.length).toBeGreaterThan(0)
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
})
