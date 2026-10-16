import { createWidgetDefaults } from '@/lib/widget/widget-defaults'
import { describe, test, expect, vi, beforeAll } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import HeadingTapeDisplaySection from '@/features/widget-editor/components/metricWidget/HeadingTapeDisplaySection'

vi.mock('@/hooks/useFonts', async () => ({
  ...(await vi.importActual('@/hooks/useFonts')),
  useAvailableFonts: () => ({
    recommendedFonts: [],
    systemFonts: [],
  }),
}))

beforeAll(() => {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
})

function makeHeadingWidget(overrides = {}) {
  const data = createWidgetDefaults({ type: 'heading', displayType: 'heading_tape' })
  return {
    id: 'plot-0',
    type: 'heading',
    category: 'values',
    data: {
      ...data,
      display_variants: {
        heading_tape: { ...data.display_variants.heading_tape, ...overrides },
      },
    },
  }
}

function makeHeadingTapeWidget(tapeOverrides = {}) {
  return makeHeadingWidget(tapeOverrides)
}

describe('HeadingTapeDisplaySection', () => {
  test('renders the Tape section with pixels_per_degree control', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget()} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Tape Scale')).toBeInTheDocument()
    expect(screen.getByText('Pixels per Degree')).toBeInTheDocument()
  })

  test('renders the Ticks section with major/minor toggles', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget()} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Ticks')).toBeInTheDocument()
    expect(screen.getByText('Major Ticks')).toBeInTheDocument()
    expect(screen.getByText('Minor Ticks')).toBeInTheDocument()
  })

  test('renders tick length and thickness controls', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget()} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Major Length')).toBeInTheDocument()
    expect(screen.getByText('Minor Length')).toBeInTheDocument()
    expect(screen.getByText('Major Thickness')).toBeInTheDocument()
    expect(screen.getByText('Minor Thickness')).toBeInTheDocument()
  })

  test('renders tick color controls', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget()} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Tick Color')).toBeInTheDocument()
    const cardinalColors = screen.getAllByText('Cardinal Color')
    expect(cardinalColors.length).toBeGreaterThanOrEqual(1)
  })

  test('renders tick alignment dropdown', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget()} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Alignment')).toBeInTheDocument()
  })

  test('renders the Labels section with show/hide toggles', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget()} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Labels')).toBeInTheDocument()
    expect(screen.getByText('Minor Labels')).toBeInTheDocument()
    expect(screen.getByText('Major Labels')).toBeInTheDocument()
  })

  test('renders label color and font size controls', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget()} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Label Font')).toBeInTheDocument()
    expect(screen.getByText('Label Color')).toBeInTheDocument()
    expect(screen.getAllByText('Cardinal Color').length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText('Font Size')).toBeInTheDocument()
  })

  test('renders the Indicator section with style and placement controls', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget()} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Indicator')).toBeInTheDocument()
    expect(screen.getByText('Show Indicator')).toBeInTheDocument()
    expect(screen.getByText('Style')).toBeInTheDocument()
    expect(screen.getByText('Placement')).toBeInTheDocument()
  })

  test('renders indicator color and size controls', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget()} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Indicator Color')).toBeInTheDocument()
    expect(screen.getByText('Indicator Size')).toBeInTheDocument()
  })

  test('major tick toggle writes to display_variants.heading_tape', async () => {
    const updateWidgetData = vi.fn()
    const user = userEvent.setup()
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget({ show_major_ticks: true })} updateWidgetData={updateWidgetData} />)

    const majorTicksLabel = screen.getByText('Major Ticks')
    const toggle = majorTicksLabel.closest('div').querySelector('button')
    await user.click(toggle)

    expect(updateWidgetData).toHaveBeenCalledWith(
      'plot-0',
      expect.objectContaining({
        display_variants: expect.objectContaining({
          heading_tape: expect.objectContaining({ show_major_ticks: false }),
        }),
      }),
    )
  })

  test('indicator style select shows current value', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget({ indicator_style: 'highlight_bar' })} updateWidgetData={vi.fn()} />)
    expect(screen.getByText('Highlight Bar')).toBeInTheDocument()
  })

  test('disables major tick options when major ticks are hidden', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget({ show_major_ticks: false })} updateWidgetData={vi.fn()} />)

    const majorLengthSlider = screen.getByText('Major Length').closest('div').parentElement.querySelector('[data-slot="slider"]')
    const majorThicknessSlider = screen.getByText('Major Thickness').closest('div').parentElement.querySelector('[data-slot="slider"]')

    expect(majorLengthSlider).toHaveAttribute('aria-disabled', 'true')
    expect(majorThicknessSlider).toHaveAttribute('aria-disabled', 'true')
  })

  test('disables minor tick options when minor ticks are hidden', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget({ show_minor_ticks: false })} updateWidgetData={vi.fn()} />)

    const minorLengthSlider = screen.getByText('Minor Length').closest('div').parentElement.querySelector('[data-slot="slider"]')
    const minorThicknessSlider = screen.getByText('Minor Thickness').closest('div').parentElement.querySelector('[data-slot="slider"]')

    expect(minorLengthSlider).toHaveAttribute('aria-disabled', 'true')
    expect(minorThicknessSlider).toHaveAttribute('aria-disabled', 'true')
  })

  test('disables indicator placement when style is highlight bar', () => {
    render(<HeadingTapeDisplaySection widget={makeHeadingTapeWidget({ indicator_style: 'highlight_bar' })} updateWidgetData={vi.fn()} />)

    const placementTrigger = screen.getByText('Placement').closest('div').parentElement.querySelector('[data-slot="select-trigger"]')

    expect(placementTrigger).toBeDisabled()
  })

  test('tape defaults match the PRD spec', () => {
    const defaults = createWidgetDefaults({ type: 'heading', displayType: 'heading_tape' }).display_variants.heading_tape
    expect(defaults).toMatchObject({
      major_tick_interval: 15,
      minor_ticks_per_major: 3,
      show_major_ticks: true,
      show_minor_ticks: true,
      major_tick_thickness: 2,
      minor_tick_thickness: 2,
      label_font_size: 12,
      tick_alignment: 'below',
      indicator_style: 'chevron',
      show_indicator: true,
    })
  })

  test('widget data serializes cleanly (no undefined values in defaults)', () => {
    const defaults = createWidgetDefaults({ type: 'heading', displayType: 'heading_tape' }).display_variants.heading_tape
    Object.entries(defaults).forEach(([key, value]) => {
      expect(value).not.toBeUndefined()
      expect(key).not.toContain('undefined')
    })
  })
})
