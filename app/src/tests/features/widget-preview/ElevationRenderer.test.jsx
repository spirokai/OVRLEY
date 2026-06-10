import { describe, expect, test, vi, beforeEach } from 'vitest'
import { render, waitFor } from '@testing-library/react'

// Mock the backend IPC call
const mockBuildElevationGeometry = vi.fn()
vi.mock('@/api/backend', () => ({
  buildElevationGeometry: (...args) => mockBuildElevationGeometry(...args),
  hasTauriRuntime: () => true,
}))

import { OverlayElevationWidget } from '@/features/widget-preview'

const GEOMETRY_RESPONSE = {
  points: [
    [0, 48],
    [80, 24],
    [160, 36],
    [240, 0],
  ],
  progressValues: [0, 0.33, 0.66, 1],
  bbox: [0, 0, 240, 48],
  sourcePointCount: 4,
  simplification: 'sg11_density_1.00_rdp_px_1.00',
  widgetWidth: 240,
  widgetHeight: 48,
}

function makeElevationWidget(overrides = {}) {
  return {
    id: 'test-elevation-1',
    type: 'elevation',
    category: 'plots',
    data: {
      x: 100,
      y: 200,
      width: 240,
      height: 48,
      color: '#ffffff',
      remaining_line_width: 2,
      completed_line_width: 2,
      marker_size: 16,
      show_elevation_metric: true,
      show_elevation_imperial: false,
      point_label: {
        font_size: 12,
      },
      ...overrides,
    },
  }
}

function makeActivity() {
  return {
    sample_elapsed_seconds: [0, 10, 20, 30],
    sample_distance_progress: [0, 0.33, 0.66, 1],
    sample_elevations: [100, 130, 115, 160],
    elevation: [100, 130, 115, 160],
  }
}

describe('OverlayElevationWidget', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockBuildElevationGeometry.mockResolvedValue(GEOMETRY_RESPONSE)
  })

  test('uses the widget height as the SVG coordinate height below the old clamp threshold', async () => {
    const widget = makeElevationWidget({ height: 48 })
    const { container } = render(
      <OverlayElevationWidget
        widget={widget}
        activity={makeActivity()}
        previewSecond={15}
        globalOpacity={1}
        globalScale={1}
        sceneFont="Inter"
        sceneFontSize={12}
        sceneStyle={{}}
      />,
    )

    await waitFor(() => {
      const svg = container.querySelector('svg')
      expect(svg).toBeTruthy()
    })

    const svg = container.querySelector('svg')
    const marker = container.querySelector('circle')
    const line = container.querySelector('polyline')

    expect(svg).toHaveAttribute('height', '48')
    expect(svg).toHaveAttribute('viewBox', '0 0 240 48')
    expect(marker).toHaveAttribute('r', '16')
    expect(line).toHaveAttribute('stroke-width', '2')
  })
})
