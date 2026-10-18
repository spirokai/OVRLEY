import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { beforeAll, describe, expect, test, vi } from 'vite-plus/test'
import { fireEvent, render, screen } from '@testing-library/react'

import ElevationWidgetEditor from '@/features/widget-editor/components/ElevationWidgetEditor'
import DivingProfileWidgetEditor from '@/features/widget-editor/components/DivingProfileWidgetEditor'
import RouteMapWidgetEditor from '@/features/widget-editor/components/RouteMapWidgetEditor'

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
      ...WIDGET_TYPE_DEFINITIONS.course.defaults,
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
      ...WIDGET_TYPE_DEFINITIONS.elevation.defaults,
      ...overrides,
    },
  }
}

describe('plot marker variant controls', () => {
  test('diving profile exposes ceiling, depth, and geometry controls without an altitude override', () => {
    const update = vi.fn()
    const resize = vi.fn()
    const commit = vi.fn()
    render(
      <DivingProfileWidgetEditor
        widget={{ id: 'dive', type: 'diving_profile', data: { ...WIDGET_TYPE_DEFINITIONS.diving_profile.defaults, marker_variant: 'halo' } }}
        updateWidgetData={update}
        setNumericField={vi.fn()}
        updateWidgetSize={resize}
        commitWidgetSize={commit}
      />,
    )
    expect(screen.getByText('Decompression Ceiling')).toBeInTheDocument()
    expect(screen.getByText('Halo Diameter')).toBeInTheDocument()
    const smoothing = screen.getByRole('spinbutton', { name: 'Smoothing' })
    expect(smoothing).toHaveValue(1.5)
    fireEvent.change(smoothing, { target: { value: '2.5' } })
    fireEvent.blur(smoothing)
    expect(resize).toHaveBeenCalledWith('dive', { simplify_tolerance_px: 2.5 })
    const detail = screen.getByRole('spinbutton', { name: 'Profile Detail' })
    expect(detail).toHaveValue(0.5)
    fireEvent.change(detail, { target: { value: '1.25' } })
    fireEvent.blur(detail)
    expect(resize).toHaveBeenCalledWith('dive', { target_density: 1.25 })
    expect(commit).toHaveBeenCalledWith('dive')
    expect(screen.queryByText('Elevation at start')).not.toBeInTheDocument()
    const metric = screen.getByRole('switch', { name: 'Label Metric' })
    expect(metric).toBeChecked()
    fireEvent.click(metric)
    expect(update).toHaveBeenCalledWith('dive', { show_depth_metric: false })
  })
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
