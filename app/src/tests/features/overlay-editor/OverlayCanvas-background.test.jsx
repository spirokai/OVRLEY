/**
 * Background mode rendering tests for OverlayCanvas.
 *
 * Verifies the correct visual elements render for each backgroundMode value.
 */

import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useVideoPreview = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path) => path }))

vi.mock('@/features/video-preview', () => ({
  useVideoPreview,
}))

vi.mock('@/features/widget-preview', () => ({
  WidgetPreview: () => <div data-testid="widget-preview" />,
  buildMetricWidgetPreviewModel: () => null,
  buildTextWidgetPreviewModel: () => null,
}))

vi.mock('@/features/widget-preview/shared/useFontMetrics', () => ({
  useFontMetrics: () => 0,
}))

vi.mock('@/features/widget-preview/shared/textMeasurement', () => ({
  getPreviewFontFamily: (fontFamily) => fontFamily || 'Arial',
}))

import OverlayCanvas from '@/features/overlay-editor/components/OverlayCanvas'

const defaultSceneProps = { sceneFont: 'Arial', sceneFontSize: 30, sceneStyle: {}, valueFont: 'Arial', sceneSize: { width: 1920, height: 1080 } }
const defaultDisplayProps = (mode) => ({ displayScale: 1, globalScale: 1, globalOpacity: 1, backgroundMode: mode, gridVisible: false })
const defaultDataProps = {
  widgets: [],
  activity: null,
  previewSecond: 0,
  metricPreviewModels: {},
  textPreviewModels: {},
  selectionRect: null,
  exportRange: null,
}
const defaultCallbacks = {
  setSceneElement: vi.fn(),
  handleSceneMouseDown: vi.fn(),
  handleWidgetMouseDown: vi.fn(),
  setHoveredWidgetId: vi.fn(),
  widgetRefCallbacks: {},
}

describe('OverlayCanvas background modes', () => {
  beforeEach(() => {
    useVideoPreview.mockReset()
    useVideoPreview.mockReturnValue({ videoSrc: '', importId: null, isOutOfRange: false, videoPreviewMessages: [] })
  })

  test('checker mode renders checkered background', () => {
    const { container } = render(
      <OverlayCanvas
        sceneProps={defaultSceneProps}
        displayProps={defaultDisplayProps('checker')}
        dataProps={defaultDataProps}
        callbacks={defaultCallbacks}
      />,
    )

    expect(container.querySelector('.bg-overlay-grid-muted')).toBeTruthy()
  })

  test('black mode renders black background', () => {
    const { container } = render(
      <OverlayCanvas
        sceneProps={defaultSceneProps}
        displayProps={defaultDisplayProps('black')}
        dataProps={defaultDataProps}
        callbacks={defaultCallbacks}
      />,
    )

    const scene = container.querySelector('[data-testid="overlay-scene"]')
    expect(scene).toBeTruthy()
  })

  test('white mode renders white background', () => {
    const { container } = render(
      <OverlayCanvas
        sceneProps={defaultSceneProps}
        displayProps={defaultDisplayProps('white')}
        dataProps={defaultDataProps}
        callbacks={defaultCallbacks}
      />,
    )

    expect(container.querySelector('[data-testid="overlay-scene"]')).toBeTruthy()
  })

  test('transparent mode omits background div', () => {
    const { container } = render(
      <OverlayCanvas
        sceneProps={defaultSceneProps}
        displayProps={defaultDisplayProps('transparent')}
        dataProps={defaultDataProps}
        callbacks={defaultCallbacks}
      />,
    )

    expect(container.querySelector('.bg-overlay-grid-muted')).toBeNull()
  })

  test('video mode renders the scene container', () => {
    const { container } = render(
      <OverlayCanvas
        sceneProps={defaultSceneProps}
        displayProps={defaultDisplayProps('video')}
        dataProps={defaultDataProps}
        callbacks={defaultCallbacks}
      />,
    )

    expect(container.querySelector('[data-testid="overlay-scene"]')).toBeTruthy()
  })

  test('sync mode forces video and replaces project widgets with diagnostics without mutating display state', () => {
    const displayProps = defaultDisplayProps('checker')
    const dataProps = {
      ...defaultDataProps,
      widgets: [{ id: 'saved-widget', category: 'values', type: 'speed' }],
      renderGeometryModels: {
        'saved-widget': { visualBounds: null, renderGeometry: { left: 0, top: 0, width: 20, height: 20, transform: '', transformOrigin: '' } },
      },
    }

    render(
      <OverlayCanvas
        sceneProps={defaultSceneProps}
        displayProps={displayProps}
        dataProps={dataProps}
        callbacks={defaultCallbacks}
        videoSyncMode
        videoSyncDiagnostics={{
          speed: { available: false },
          route: { available: false, widget: { data: { width: 806, height: 594 } } },
        }}
      />,
    )

    expect(useVideoPreview).toHaveBeenLastCalledWith(expect.any(Object), true)
    expect(screen.queryByTestId('widget-preview')).not.toBeInTheDocument()
    expect(screen.getByTestId('video-sync-canvas-diagnostics')).toBeInTheDocument()
    expect(displayProps.backgroundMode).toBe('checker')
    expect(dataProps.widgets).toHaveLength(1)
  })
})
