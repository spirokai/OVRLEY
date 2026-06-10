import { describe, expect, test } from 'vitest'

import { getPreviewMarkerLayers, buildElevationCompletedPoints } from '@/features/widget-preview/utils/svgPreviewUtils'

describe('getPreviewMarkerLayers', () => {
  test('builds a single solid marker by default', () => {
    const layers = getPreviewMarkerLayers({}, 16, '#ffffff', 1)

    expect(layers).toHaveLength(1)
    expect(layers[0]).toMatchObject({
      radius: 16,
      color: '#ffffff',
      opacity: 1,
      solidFill: true,
    })
  })

  test('adds a thin concentric ring marker layer', () => {
    const layers = getPreviewMarkerLayers(
      {
        marker_variant: 'ring',
        marker_variant_diameter: 44,
      },
      18,
      '#40e0d0',
      0.8,
    )

    expect(layers).toHaveLength(2)
    expect(layers[0]).toMatchObject({
      radius: 22,
      color: '#40e0d0',
      opacity: 0.8,
      solidFill: false,
      strokeWidth: 1.5,
    })
    expect(layers[1].solidFill).toBe(true)
  })

  test('adds a semi-transparent halo underneath the main marker', () => {
    const layers = getPreviewMarkerLayers(
      {
        marker_variant: 'halo',
        marker_variant_diameter: 52,
      },
      16,
      '#ff6600',
      0.6,
    )

    expect(layers).toHaveLength(2)
    expect(layers[0]).toMatchObject({
      radius: 26,
      color: '#ff6600',
      opacity: 0.21,
      solidFill: true,
    })
    expect(layers[1]).toMatchObject({
      radius: 16,
      solidFill: true,
    })
  })
})

describe('buildElevationCompletedPoints', () => {
  test('filters the geometry by elapsed fraction and ends on the profile geometry', () => {
    const points = [
      [0, 50],
      [20, 48],
      [40, 45],
      [60, 40],
      [80, 35],
      [100, 30],
    ]
    const progressValues = [0, 0.2, 0.4, 0.6, 0.8, 1]
    const elapsedFractions = [0, 0.2, 0.4, 0.6, 0.8, 1]
    const completed = buildElevationCompletedPoints(points, progressValues, elapsedFractions, 0.5, 0.5)

    expect(completed).toHaveLength(4)
    expect(completed[0]).toEqual([0, 50])
    expect(completed[1]).toEqual([20, 48])
    expect(completed[2]).toEqual([40, 45])
    expect(completed[3]).toEqual([50, 42.5])
  })

  test('keeps a chronological vertical prefix when progress is duplicated', () => {
    const points = [
      [30, 50],
      [30, 45],
      [30, 40],
      [30, 35],
      [60, 30],
    ]
    const progressValues = [0.5, 0.5, 0.5, 0.5, 1]
    const elapsedFractions = [0, 0.25, 0.5, 0.75, 1]
    const completed = buildElevationCompletedPoints(points, progressValues, elapsedFractions, 0.5, 0.5)

    expect(completed).toEqual([
      [30, 50],
      [30, 45],
      [30, 40],
    ])
  })

  test('includes all points at full elapsed fraction', () => {
    const points = [
      [0, 50],
      [40, 45],
      [80, 35],
    ]
    const progressValues = [0, 0.5, 1]
    const elapsedFractions = [0, 0.5, 1]
    const completed = buildElevationCompletedPoints(points, progressValues, elapsedFractions, 1, 1)

    expect(completed).toHaveLength(3)
  })

  test('returns empty array for empty points', () => {
    expect(buildElevationCompletedPoints([], [], [], 0.5, 0.5)).toEqual([])
  })

  test('includes first point when no points match elapsed fraction', () => {
    const points = [
      [0, 50],
      [20, 48],
    ]
    const progressValues = [0.6, 0.8]
    const elapsedFractions = [0.6, 0.8]
    const completed = buildElevationCompletedPoints(points, progressValues, elapsedFractions, 0.5, 0.5)

    expect(completed.length).toBeGreaterThanOrEqual(1)
    expect(completed[0]).toEqual([0, 50])
  })

  test('does not let elapsed-time fill outrun the distance-based marker in normal motion', () => {
    const points = [
      [0, 50],
      [20, 48],
      [40, 45],
      [60, 40],
    ]
    const progressValues = [0, 0.2, 0.4, 0.6]
    const elapsedFractions = [0, 0.1, 0.9, 1]

    const completed = buildElevationCompletedPoints(points, progressValues, elapsedFractions, 0.3, 0.95)

    expect(completed).toEqual([
      [0, 50],
      [20, 48],
      [30, 46.5],
    ])
  })
})
