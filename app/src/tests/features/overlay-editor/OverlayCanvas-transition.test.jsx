/**
 * The incoming overlay of a transition window in OverlayCanvas.
 */

import { render, screen, within } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path) => path }))

vi.mock('@/features/video-preview', () => ({
  VideoPreviewSurface: () => <div data-testid="video-preview-surface" />,
}))

vi.mock('@/features/widget-preview', () => ({
  WidgetPreview: ({ widget, previewSecond, activity }) => (
    <div data-testid="widget-preview">{`${widget.id}@${previewSecond}:${activity?.name ?? 'none'}`}</div>
  ),
}))

import OverlayCanvas from '@/features/overlay-editor/components/OverlayCanvas'

const geometry = { renderGeometry: { left: 0, top: 0, width: 10, height: 10, transform: 'none', transformOrigin: 'center' }, visualBounds: null }
const widgets = [
  { id: 'speed', type: 'value', category: 'values', data: {} },
  { id: 'logo', type: 'raster', category: 'rasters', data: {} },
]

function renderCanvas(incomingOverlay) {
  return render(
    <OverlayCanvas
      sceneProps={{ sceneFont: 'Arial', sceneFontSize: 30, sceneStyle: {}, valueFont: 'Arial', sceneSize: { width: 1920, height: 1080 } }}
      displayProps={{ displayScale: 1, globalScale: 1, globalOpacity: 1, backgroundMode: 'black', gridVisible: false }}
      dataProps={{
        widgets,
        activity: { name: 'outgoing' },
        previewSecond: 59,
        metricPreviewModels: {},
        textPreviewModels: {},
        renderGeometryModels: { speed: geometry, logo: geometry },
        exportRange: null,
        hasActivityCoverage: true,
        incomingOverlay,
      }}
      callbacks={{ setSceneElement: vi.fn(), handleWidgetMouseDown: vi.fn(), setHoveredWidgetId: vi.fn(), widgetRefCallbacks: {} }}
    />,
  )
}

describe('OverlayCanvas transitions', () => {
  test('blends the editable layer into the incoming widgets at their own time', () => {
    renderCanvas({
      progress: 0.25,
      activity: { name: 'incoming' },
      previewSecond: 1,
      hasActivityCoverage: false,
      exportRange: null,
      metricPreviewModels: {},
      textPreviewModels: {},
    })
    expect(screen.getByTestId('widget-layer')).toHaveStyle({ opacity: '0.75' })
    expect(
      within(screen.getByTestId('widget-layer'))
        .getAllByTestId('widget-preview')
        .map((node) => node.textContent),
    ).toEqual(['speed@59:outgoing', 'logo@59:outgoing'])
    const incoming = screen.getByTestId('incoming-widget-layer')
    expect(incoming).toHaveStyle({ opacity: '0.25', mixBlendMode: 'plus-lighter' })
    const previews = within(incoming).getAllByTestId('widget-preview')
    expect(previews.map((node) => node.textContent)).toEqual(['speed@1:incoming', 'logo@1:incoming'])
    // Without activity coverage at the incoming time only rasters show.
    expect(previews[0].parentElement).toHaveClass('invisible')
    expect(previews[1].parentElement).not.toHaveClass('invisible')
  })

  test('shows only the editable layer outside transitions', () => {
    renderCanvas(null)
    expect(screen.queryByTestId('incoming-widget-layer')).not.toBeInTheDocument()
    expect(screen.getByTestId('widget-layer').style.opacity).toBe('')
  })
})
