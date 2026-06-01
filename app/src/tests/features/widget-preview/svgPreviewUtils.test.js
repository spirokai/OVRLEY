import { describe, expect, test } from 'vitest'

import { getPreviewMarkerLayers } from '@/features/widget-preview/utils/svgPreviewUtils'

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
