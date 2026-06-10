import { describe, expect, test, vi, beforeEach } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'

// Mock the backend IPC call before importing the hook
const mockBuildElevationGeometry = vi.fn()
vi.mock('@/api/backend', () => ({
  buildElevationGeometry: (...args) => mockBuildElevationGeometry(...args),
  hasTauriRuntime: () => true,
}))

// Mock the Zustand store — the hook reads config from here
const mockConfig = {
  scene: {
    width: 240,
    height: 48,
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
      value: 'elevation',
      x: 100,
      y: 200,
      width: 240,
      height: 48,
      y_scale: 1,
      simplify_tolerance_px: 1,
      target_density: 1,
      show_full_activity: true,
    },
  ],
}

vi.mock('@/store/useStore', () => ({
  default: vi.fn((selector) => selector({ config: mockConfig })),
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
  getPointAtProgress: vi.fn(() => null),
  pointsToSvg: vi.fn((points) => points.map((p) => p.join(',')).join(' ')),
  areaToSvg: vi.fn((points, width, height) => {
    if (!points.length) return ''
    const baseline = height
    const topPoints = points.map(([x, y]) => `${x},${y}`).join(' ')
    return `0,${baseline} ${topPoints} ${width},${baseline}`
  }),
}))

// Mock svgPreviewUtils — completed points filtering
vi.mock('@/features/widget-preview/utils/svgPreviewUtils', () => ({
  buildElevationCompletedPoints: vi.fn((points, progress, progress01, _marker) => {
    return points.filter((_, i) => (progress[i] ?? 0) <= progress01)
  }),
  getPreviewMarkerLayers: vi.fn(() => []),
  sanitizeSvgId: vi.fn((id) => id),
}))

import { useElevationPreviewGeometry } from '@/features/widget-preview/hooks/useElevationPreviewGeometry'

function makeActivity() {
  return {
    sample_elapsed_seconds: [0, 10, 20, 30],
    sample_distance_progress: [0, 0.33, 0.66, 1],
    sample_elevations: [100, 130, 115, 160],
    elevation: [100, 130, 115, 160],
  }
}

function makeData() {
  return {
    x: 100,
    y: 200,
    width: 240,
    height: 48,
    y_scale: 1,
    target_density: 1,
    simplify_tolerance_px: 1,
    show_full_activity: true,
    show_elevation_metric: true,
    show_elevation_imperial: false,
  }
}

function makeStyle() {
  return { width: 240, height: 48, safeGlobalScale: 1 }
}

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

describe('useElevationPreviewGeometry', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockBuildElevationGeometry.mockResolvedValue(GEOMETRY_RESPONSE)
  })

  test('calls buildElevationGeometry with store config and activity', async () => {
    const activity = makeActivity()

    renderHook(() =>
      useElevationPreviewGeometry({
        activity,
        data: makeData(),
        exportRange: null,
        previewSecond: 15,
        style: makeStyle(),
      }),
    )

    await waitFor(() => {
      expect(mockBuildElevationGeometry).toHaveBeenCalledTimes(1)
    })

    const [config, passedActivity] = mockBuildElevationGeometry.mock.calls[0]
    expect(passedActivity).toBe(activity)
    // Should have scene.start and scene.end filled from activity duration
    expect(config.scene.start).toBe(0)
    expect(config.scene.end).toBe(30)
    // Should preserve the original scene dimensions
    expect(config.scene.width).toBe(240)
    expect(config.scene.height).toBe(48)
    // Should contain the elevation plot
    expect(config.plots).toEqual(expect.arrayContaining([expect.objectContaining({ value: 'elevation' })]))
  })

  test('returns geometry with correct output shape after IPC resolves', async () => {
    const { result } = renderHook(() =>
      useElevationPreviewGeometry({
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
    expect(geometry).toHaveProperty('elevationValue')
    expect(geometry).toHaveProperty('remainingSvgPoints')
    expect(geometry).toHaveProperty('completedSvgPoints')
    expect(geometry).toHaveProperty('areaSvgPoints')
    expect(geometry).toHaveProperty('completedAreaSvgPoints')

    expect(Array.isArray(geometry.markerPoint)).toBe(true)
    expect(geometry.markerPoint).toHaveLength(2)
    expect(typeof geometry.remainingSvgPoints).toBe('string')
    expect(geometry.remainingSvgPoints.length).toBeGreaterThan(0)
  })

  test('returns null while IPC call is in flight', () => {
    mockBuildElevationGeometry.mockReturnValue(new Promise(() => {}))

    const { result } = renderHook(() =>
      useElevationPreviewGeometry({
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
