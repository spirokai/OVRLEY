import { describe, expect, test, vi, beforeEach } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'

// Mock the backend IPC call before importing the hook
const mockBuildRouteGeometry = vi.fn()
vi.mock('@/api/backend', () => ({
  buildRouteGeometry: (...args) => mockBuildRouteGeometry(...args),
  hasTauriRuntime: () => true,
}))

// Mock the Zustand store — the hook reads config from here
const mockConfig = {
  scene: {
    width: 240,
    height: 240,
    fps: 30,
    start: 0,
    end: 30,
    scale: 1,
    shadow_color: '#000000',
    shadow_strength: 0,
    shadow_distance: 0,
    border_color: '#000000',
    border_thickness: 0,
    update_rate: 1,
    custom_export_range_active: false,
    ffmpeg: {},
  },
  values: [],
  labels: [],
  plots: [
    {
      value: 'course',
      x: 0,
      y: 0,
      width: 240,
      height: 240,
      simplify_tolerance_px: 1,
      target_density: 1,
      show_full_activity: true,
      remaining_line_width: 2,
      completed_line_width: 2,
      marker_size: 6,
    },
  ],
}

vi.mock('@/store/useStore', () => ({
  default: vi.fn((selector) => selector({ config: mockConfig, globalDefaults: {}, fallbackDurationSeconds: 73 })),
}))

// Mock geometryUtils — the hook still uses local marker interpolation
vi.mock('@/lib/geometryUtils', () => ({
  getPointAtMetricProgress: vi.fn((points, progress, target) => {
    if (!points.length) return null
    for (let i = 0; i < points.length - 1; i++) {
      if (target >= progress[i] && target <= progress[i + 1]) {
        const t = (target - progress[i]) / (progress[i + 1] - progress[i] || 1)
        return [points[i][0] + t * (points[i + 1][0] - points[i][0]), points[i][1] + t * (points[i + 1][1] - points[i][1])]
      }
    }
    return points[points.length - 1]
  }),
  pointsToSvg: vi.fn((points) => points.map((p) => p.join(',')).join(' ')),
}))

// Mock svgPreviewUtils — completed points filtering
vi.mock('@/features/widget-preview/utils/svgPreviewUtils', () => ({
  buildRouteFramePreview: vi.fn((points, progressValues, progress01) => {
    if (!points.length) return { markerPoint: null, completedPoints: [] }
    const lastPoint = points[points.length - 1]
    const completedPoints = points.filter((_, i) => (progressValues[i] ?? 0) <= progress01)
    return { markerPoint: lastPoint, completedPoints }
  }),
  sanitizeSvgId: vi.fn((id) => id),
}))

import { useRoutePreviewGeometry } from '@/features/widget-preview/hooks/useRoutePreviewGeometry'

function makeActivity() {
  return {
    sample_elapsed_seconds: [0, 10, 20, 30],
    sample_distance_progress: [0, 0.33, 0.66, 1],
    sample_course_points: [
      [47.6062, -122.3321],
      [47.6065, -122.3325],
      [47.6068, -122.333],
      [47.607, -122.3335],
    ],
  }
}

function makeData() {
  return {
    x: 0,
    y: 0,
    width: 240,
    height: 240,
    target_density: 1,
    simplify_tolerance_px: 1,
    show_full_activity: true,
    remaining_line_width: 2,
    completed_line_width: 2,
    marker_size: 6,
  }
}

function makeStyle() {
  return { width: 240, height: 240, safeGlobalScale: 1, remainingLineWidth: 2, completedLineWidth: 2, routeMarkerInsetRadius: 6 }
}

const GEOMETRY_RESPONSE = {
  points: [
    [10, 230],
    [80, 160],
    [160, 80],
    [230, 10],
  ],
  progressValues: [0, 0.33, 0.66, 1],
  bbox: [0, 0, 240, 240],
  sourcePointCount: 4,
  simplification: 'lttb_density_1.00_rdp_px_1.00',
  widgetWidth: 240,
  widgetHeight: 240,
}

describe('useRoutePreviewGeometry', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockBuildRouteGeometry.mockResolvedValue(GEOMETRY_RESPONSE)
  })

  test('calls buildRouteGeometry with store config and activity', async () => {
    const activity = makeActivity()

    renderHook(() =>
      useRoutePreviewGeometry({
        activity,
        data: makeData(),
        exportRange: null,
        previewSecond: 15,
        style: makeStyle(),
      }),
    )

    await waitFor(() => {
      expect(mockBuildRouteGeometry).toHaveBeenCalledTimes(1)
    })

    const [config, passedActivity] = mockBuildRouteGeometry.mock.calls[0]
    expect(passedActivity).toBe(activity)
    // Should have scene.start and scene.end filled from activity duration
    expect(config.scene.start).toBe(0)
    expect(config.scene.end).toBe(30)
    // Should preserve the original scene dimensions
    expect(config.scene.width).toBe(240)
    expect(config.scene.height).toBe(240)
    // Should contain the course plot
    expect(config.plots).toEqual(expect.arrayContaining([expect.objectContaining({ value: 'course' })]))
  })

  test('returns geometry with correct output shape after IPC resolves', async () => {
    const { result } = renderHook(() =>
      useRoutePreviewGeometry({
        activity: makeActivity(),
        data: makeData(),
        exportRange: null,
        previewSecond: 15,
        style: makeStyle(),
      }),
    )

    await waitFor(() => {
      expect(result.current).not.toBeNull()
    })

    const geometry = result.current
    expect(geometry).toHaveProperty('markerPoint')
    expect(geometry).toHaveProperty('remainingSvgPoints')
    expect(geometry).toHaveProperty('completedSvgPoints')

    expect(Array.isArray(geometry.markerPoint)).toBe(true)
    expect(geometry.markerPoint).toHaveLength(2)
    expect(typeof geometry.remainingSvgPoints).toBe('string')
    expect(geometry.remainingSvgPoints.length).toBeGreaterThan(0)
  })

  test('returns null while IPC call is in flight', () => {
    mockBuildRouteGeometry.mockReturnValue(new Promise(() => {}))

    const { result } = renderHook(() =>
      useRoutePreviewGeometry({
        activity: makeActivity(),
        data: makeData(),
        exportRange: null,
        previewSecond: 15,
        style: makeStyle(),
      }),
    )

    expect(result.current).toBeNull()
  })
})
