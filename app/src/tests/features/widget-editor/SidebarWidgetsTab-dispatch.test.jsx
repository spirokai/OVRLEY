/**
 * Widget type dispatch test for SidebarWidgetsTab.
 *
 * Tests that the correct editor component renders for each widget type.
 * After refactoring the if-else chain to a dispatch map, these same tests
 * must pass identically.
 */

import { render } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

// Mock all editor components
vi.mock('@/features/widget-editor/components/TextWidgetEditor', () => ({ default: () => <div data-testid="editor-text" /> }))
vi.mock('@/features/widget-editor/components/metricWidget/MetricWidgetEditor', () => ({ default: () => <div data-testid="editor-metric" /> }))
vi.mock('@/features/widget-editor/components/TimeWidgetEditor', () => ({ default: () => <div data-testid="editor-time" /> }))
vi.mock('@/features/widget-editor/components/GradientWidgetEditor', () => ({ default: () => <div data-testid="editor-gradient" /> }))
vi.mock('@/features/widget-editor/components/RouteMapWidgetEditor', () => ({ default: () => <div data-testid="editor-course" /> }))
vi.mock('@/features/widget-editor/components/ElevationWidgetEditor', () => ({ default: () => <div data-testid="editor-elevation" /> }))

// Mock useWidgetManager with different widget types
const mockUseWidgetManager = vi.fn()

vi.mock('@/features/widget-editor/hooks/useWidgetManager', () => ({
  useWidgetManager: () => mockUseWidgetManager(),
}))

vi.mock('@/features/widget-editor/components/widgetEditorSections', () => ({
  PositionSection: () => <div data-testid="position-section" />,
}))

import SidebarWidgetsTab from '@/features/widget-editor/components/SidebarWidgetsTab'

function makeManagerState(widget) {
  return {
    config: { scene: { width: 1920, height: 1080, fps: 30 } },
    widgets: [widget],
    selectedWidgetId: widget.id,
    updateWidgetData: vi.fn(),
    setNumericField: vi.fn(),
    deleteWidget: vi.fn(),
    resetWidget: vi.fn(),
    setSelectedWidgetId: vi.fn(),
  }
}

describe('SidebarWidgetsTab widget type dispatch', () => {
  test('label widget renders TextWidgetEditor', () => {
    mockUseWidgetManager.mockReturnValue(
      makeManagerState({ id: 'w1', type: 'label', category: 'labels', name: 'Text', data: { text: 'Hello', x: 0, y: 0 } }),
    )
    const { getByTestId } = render(<SidebarWidgetsTab />)
    expect(getByTestId('editor-text')).toBeTruthy()
  })

  test('standard metric widget (speed) renders MetricWidgetEditor', () => {
    mockUseWidgetManager.mockReturnValue(
      makeManagerState({ id: 'w1', type: 'speed', category: 'values', name: 'Speed', data: { value: 'speed', x: 0, y: 0 } }),
    )
    const { getByTestId } = render(<SidebarWidgetsTab />)
    expect(getByTestId('editor-metric')).toBeTruthy()
  })

  test('time widget renders TimeWidgetEditor', () => {
    mockUseWidgetManager.mockReturnValue(
      makeManagerState({ id: 'w1', type: 'time', category: 'values', name: 'Time', data: { value: 'time', x: 0, y: 0 } }),
    )
    const { getByTestId } = render(<SidebarWidgetsTab />)
    expect(getByTestId('editor-time')).toBeTruthy()
  })

  test('gradient widget renders GradientWidgetEditor', () => {
    mockUseWidgetManager.mockReturnValue(
      makeManagerState({ id: 'w1', type: 'gradient', category: 'plots', name: 'Gradient', data: { value: 'gradient', x: 0, y: 0 } }),
    )
    const { getByTestId } = render(<SidebarWidgetsTab />)
    expect(getByTestId('editor-gradient')).toBeTruthy()
  })

  test('course widget renders RouteMapWidgetEditor', () => {
    mockUseWidgetManager.mockReturnValue(
      makeManagerState({ id: 'w1', type: 'course', category: 'plots', name: 'Course', data: { value: 'course', x: 0, y: 0 } }),
    )
    const { getByTestId } = render(<SidebarWidgetsTab />)
    expect(getByTestId('editor-course')).toBeTruthy()
  })

  test('elevation widget renders ElevationWidgetEditor', () => {
    mockUseWidgetManager.mockReturnValue(
      makeManagerState({ id: 'w1', type: 'elevation', category: 'plots', name: 'Elevation', data: { value: 'elevation', x: 0, y: 0 } }),
    )
    const { getByTestId } = render(<SidebarWidgetsTab />)
    expect(getByTestId('editor-elevation')).toBeTruthy()
  })

  test('heading widget renders MetricWidgetEditor', () => {
    mockUseWidgetManager.mockReturnValue(
      makeManagerState({ id: 'w1', type: 'heading', category: 'values', name: 'Heading', data: { value: 'heading', x: 0, y: 0 } }),
    )
    const { getByTestId } = render(<SidebarWidgetsTab />)
    expect(getByTestId('editor-metric')).toBeTruthy()
  })
})
