import { describe, test, expect, vi, beforeAll } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import HeadingWidgetEditor from '@/features/widget-editor/components/HeadingWidgetEditor'
import { HEADING_DEFAULTS } from '@/features/widget-editor/data/widgetDefaults'

vi.mock('@/features/scene-settings/hooks/useAvailableFonts', () => ({
  default: () => ({
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
  return {
    id: 'plot-0',
    type: 'heading',
    category: 'plots',
    data: {
      ...HEADING_DEFAULTS,
      ...overrides,
    },
  }
}

describe('HeadingWidgetEditor', () => {
  test('renders the Tape section with pixels_per_degree control', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget()} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Tape Scale')).toBeInTheDocument()
    expect(screen.getByText('Pixels per Degree')).toBeInTheDocument()
  })

  test('renders the Ticks section with major/minor toggles', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget()} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Ticks')).toBeInTheDocument()
    expect(screen.getByText('Major Ticks')).toBeInTheDocument()
    expect(screen.getByText('Minor Ticks')).toBeInTheDocument()
  })

  test('renders tick length and thickness controls', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget()} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Major Length')).toBeInTheDocument()
    expect(screen.getByText('Minor Length')).toBeInTheDocument()
    expect(screen.getByText('Major Thickness')).toBeInTheDocument()
    expect(screen.getByText('Minor Thickness')).toBeInTheDocument()
  })

  test('renders tick color controls', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget()} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Tick Color')).toBeInTheDocument()
    // Cardinal tick color appears in Ticks section
    const cardinalColors = screen.getAllByText('Cardinal Color')
    expect(cardinalColors.length).toBeGreaterThanOrEqual(1)
  })

  test('renders tick alignment dropdown', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget()} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Alignment')).toBeInTheDocument()
  })

  test('renders the Labels section with show/hide toggles', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget()} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Labels')).toBeInTheDocument()
    expect(screen.getByText('Minor Labels')).toBeInTheDocument()
    expect(screen.getByText('Major Labels')).toBeInTheDocument()
  })

  test('renders label color and font size controls', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget()} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Label Font')).toBeInTheDocument()
    expect(screen.getByText('Label Color')).toBeInTheDocument()
    expect(screen.getAllByText('Cardinal Color').length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText('Font Size')).toBeInTheDocument()
  })

  test('renders the Indicator section with style and placement controls', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget()} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Indicator')).toBeInTheDocument()
    expect(screen.getByText('Show Indicator')).toBeInTheDocument()
    expect(screen.getByText('Style')).toBeInTheDocument()
    expect(screen.getByText('Placement')).toBeInTheDocument()
  })

  test('renders indicator color and size controls', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget()} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)
    expect(screen.getByText('Indicator Color')).toBeInTheDocument()
    expect(screen.getByText('Indicator Size')).toBeInTheDocument()
  })

  test('major tick toggle calls updateWidgetData with correct value', async () => {
    const updateWidgetData = vi.fn()
    const user = userEvent.setup()
    render(
      <HeadingWidgetEditor widget={makeHeadingWidget({ show_major_ticks: true })} updateWidgetData={updateWidgetData} setNumericField={vi.fn()} />,
    )

    // Find the Major Ticks toggle and click it
    const majorTicksLabel = screen.getByText('Major Ticks')
    const toggle = majorTicksLabel.closest('div').querySelector('button')
    await user.click(toggle)

    expect(updateWidgetData).toHaveBeenCalledWith('plot-0', { show_major_ticks: false })
  })

  test('indicator style select shows current value', () => {
    render(
      <HeadingWidgetEditor widget={makeHeadingWidget({ indicator_style: 'highlight_bar' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />,
    )
    // The Select component shows the current value
    expect(screen.getByText('Highlight Bar')).toBeInTheDocument()
  })

  test('disables major tick options when major ticks are hidden', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget({ show_major_ticks: false })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)

    const majorLengthSlider = screen.getByText('Major Length').closest('div').parentElement.querySelector('[data-slot="slider"]')
    const majorThicknessSlider = screen.getByText('Major Thickness').closest('div').parentElement.querySelector('[data-slot="slider"]')

    expect(majorLengthSlider).toHaveAttribute('aria-disabled', 'true')
    expect(majorThicknessSlider).toHaveAttribute('aria-disabled', 'true')
  })

  test('disables minor tick options when minor ticks are hidden', () => {
    render(<HeadingWidgetEditor widget={makeHeadingWidget({ show_minor_ticks: false })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)

    const minorLengthSlider = screen.getByText('Minor Length').closest('div').parentElement.querySelector('[data-slot="slider"]')
    const minorThicknessSlider = screen.getByText('Minor Thickness').closest('div').parentElement.querySelector('[data-slot="slider"]')

    expect(minorLengthSlider).toHaveAttribute('aria-disabled', 'true')
    expect(minorThicknessSlider).toHaveAttribute('aria-disabled', 'true')
  })

  test('disables indicator placement when style is highlight bar', () => {
    render(
      <HeadingWidgetEditor widget={makeHeadingWidget({ indicator_style: 'highlight_bar' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />,
    )

    const placementTrigger = screen.getByText('Placement').closest('div').querySelector('button')

    expect(placementTrigger).toBeDisabled()
  })

  test('defaults match the PRD spec', () => {
    expect(HEADING_DEFAULTS.major_tick_interval).toBe(15)
    expect(HEADING_DEFAULTS.minor_ticks_per_major).toBe(3)
    expect(HEADING_DEFAULTS.show_major_ticks).toBe(true)
    expect(HEADING_DEFAULTS.show_minor_ticks).toBe(true)
    expect(HEADING_DEFAULTS.major_tick_thickness).toBe(2)
    expect(HEADING_DEFAULTS.minor_tick_thickness).toBe(2)
    expect(HEADING_DEFAULTS.label_font).toBe('Arial.ttf')
    expect(HEADING_DEFAULTS.tick_alignment).toBe('below')
    expect(HEADING_DEFAULTS.indicator_style).toBe('chevron')
    expect(HEADING_DEFAULTS.show_indicator).toBe(true)
  })

  test('widget data serializes cleanly (no undefined values in defaults)', () => {
    const defaults = HEADING_DEFAULTS
    Object.entries(defaults).forEach(([key, value]) => {
      expect(value).not.toBeUndefined()
      expect(key).not.toContain('undefined')
    })
  })
})
