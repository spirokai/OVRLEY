import { beforeAll, describe, expect, test, vi } from 'vitest'
import { render, screen } from '@testing-library/react'

import ElevationWidgetEditor from '@/features/widget-editor/components/ElevationWidgetEditor'
import RouteMapWidgetEditor from '@/features/widget-editor/components/RouteMapWidgetEditor'
import { COURSE_PLOT_DEFAULTS, ELEVATION_PLOT_DEFAULTS } from '@/features/widget-editor/data/widgetDefaults'

beforeAll(() => {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
})

function makeRouteWidget(overrides = {}) {
  return {
    id: 'route-1',
    type: 'course',
    category: 'plots',
    data: {
      width: 400,
      height: 200,
      ...COURSE_PLOT_DEFAULTS,
      ...overrides,
    },
  }
}

function makeElevationWidget(overrides = {}) {
  return {
    id: 'elevation-1',
    type: 'elevation',
    category: 'plots',
    data: {
      width: 400,
      height: 200,
      point_label: {},
      ...ELEVATION_PLOT_DEFAULTS,
      ...overrides,
    },
  }
}

describe('plot marker variant controls', () => {
  test('route editor shows the variant diameter control for ring markers', () => {
    render(<RouteMapWidgetEditor widget={makeRouteWidget({ marker_variant: 'ring' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)

    expect(screen.getByText('Type')).toBeInTheDocument()
    expect(screen.getByText('Ring Diameter')).toBeInTheDocument()
  })

  test('route editor hides the variant diameter control for single markers', () => {
    render(<RouteMapWidgetEditor widget={makeRouteWidget({ marker_variant: 'single' })} updateWidgetData={vi.fn()} setNumericField={vi.fn()} />)

    expect(screen.queryByText('Ring Diameter')).not.toBeInTheDocument()
    expect(screen.queryByText('Halo Diameter')).not.toBeInTheDocument()
  })

  test('elevation editor shows the halo diameter control when halo markers are selected', () => {
    render(
      <ElevationWidgetEditor
        widget={makeElevationWidget({ marker_variant: 'halo' })}
        updateWidgetData={vi.fn()}
        setNumericField={vi.fn()}
        sceneFontSize={12}
      />,
    )

    expect(screen.getByText('Marker Type')).toBeInTheDocument()
    expect(screen.getByText('Halo Diameter')).toBeInTheDocument()
  })
})
