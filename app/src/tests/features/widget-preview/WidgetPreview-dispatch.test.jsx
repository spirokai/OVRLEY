/**
 * WidgetPreview dispatch tests.
 *
 * Verifies that preview routing is driven by display_type from the shared
 * manifest — not by widget type or legacy category heuristics.
 *
 * ## Type
 * Unit tests. Renders WidgetPreview with mocked child components and asserts
 * the correct renderer is invoked for each display_type scenario.
 */

import { describe, expect, test, vi } from 'vitest'
import { render } from '@testing-library/react'

vi.mock('@/features/widget-preview/components/TextRenderer', () => ({
  OverlayTextWidget: (props) => <div data-testid="text-renderer" data-widget-type={props.widget.type} />,
}))
vi.mock('@/features/widget-preview/components/MetricRenderer', () => ({
  OverlayMetricWidget: (props) => <div data-testid="metric-renderer" data-widget-type={props.widget.type} />,
}))
vi.mock('@/features/widget-preview/components/RouteRenderer', () => ({
  OverlayRouteWidget: () => <div data-testid="route-renderer" />,
}))
vi.mock('@/features/widget-preview/components/ElevationRenderer', () => ({
  OverlayElevationWidget: () => <div data-testid="elevation-renderer" />,
}))
vi.mock('@/features/widget-preview/components/HeadingRenderer', () => ({
  OverlayHeadingWidget: (props) => <div data-testid="heading-renderer" data-widget-type={props.widget.type} />,
}))
vi.mock('@/features/widget-preview/components/LinearGaugeRenderer', () => ({
  OverlayLinearGaugeWidget: (props) => (
    <div data-testid="linear-gauge-renderer" data-widget-type={props.widget.type} data-display-type={props.widget.data.display_type} />
  ),
}))

import WidgetPreview from '@/features/widget-preview/components/WidgetPreview'

const ACTIVITY = { sample_elapsed_seconds: [0], speed: [25] }

describe('WidgetPreview dispatch by display_type', () => {
  test('label widgets always use the text renderer', () => {
    const { getByTestId } = render(<WidgetPreview widget={{ type: 'label', category: 'labels', data: { text: 'Hi', x: 0, y: 0 } }} />)
    expect(getByTestId('text-renderer')).toBeTruthy()
  })

  test('course widgets use the route renderer', () => {
    const { getByTestId } = render(<WidgetPreview widget={{ type: 'course', category: 'plots', data: { x: 0, y: 0 } }} activity={ACTIVITY} />)
    expect(getByTestId('route-renderer')).toBeTruthy()
  })

  test('elevation widgets use the elevation renderer', () => {
    const { getByTestId } = render(<WidgetPreview widget={{ type: 'elevation', category: 'plots', data: { x: 0, y: 0 } }} activity={ACTIVITY} />)
    expect(getByTestId('elevation-renderer')).toBeTruthy()
  })

  test('heading with text display_type uses the metric (text) renderer', () => {
    const { getByTestId } = render(
      <WidgetPreview
        widget={{ type: 'heading', category: 'values', data: { display_type: 'text', x: 0, y: 0 } }}
        activity={ACTIVITY}
        metricPreviewModel={{ visualBounds: { width: 100, height: 30 } }}
      />,
    )
    expect(getByTestId('metric-renderer')).toBeTruthy()
  })

  test('heading with heading_tape display_type uses the heading renderer', () => {
    const { getByTestId } = render(
      <WidgetPreview
        widget={{ type: 'heading', category: 'values', data: { display_type: 'heading_tape', x: 0, y: 0, width: 400, height: 80 } }}
        activity={ACTIVITY}
      />,
    )
    expect(getByTestId('heading-renderer')).toBeTruthy()
  })

  test('speed with text display_type uses the metric renderer', () => {
    const { getByTestId } = render(
      <WidgetPreview
        widget={{ type: 'speed', category: 'values', data: { display_type: 'text', x: 0, y: 0 } }}
        activity={ACTIVITY}
        metricPreviewModel={{ visualBounds: { width: 100, height: 30 } }}
      />,
    )
    expect(getByTestId('metric-renderer')).toBeTruthy()
  })

  test('metric widget with no display_type defaults to metric renderer', () => {
    const { getByTestId } = render(
      <WidgetPreview
        widget={{ type: 'speed', category: 'values', data: { x: 0, y: 0 } }}
        activity={ACTIVITY}
        metricPreviewModel={{ visualBounds: { width: 100, height: 30 } }}
      />,
    )
    expect(getByTestId('metric-renderer')).toBeTruthy()
  })

  test('linear display_type uses the linear gauge renderer', () => {
    const { getByTestId } = render(
      <WidgetPreview
        widget={{ type: 'speed', category: 'values', data: { display_type: 'linear', x: 0, y: 0, width: 200, height: 60 } }}
        activity={ACTIVITY}
      />,
    )
    expect(getByTestId('linear-gauge-renderer')).toBeTruthy()
  })
})
