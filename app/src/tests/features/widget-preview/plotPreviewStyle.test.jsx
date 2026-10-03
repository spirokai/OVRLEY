import { describe, expect, test, vi } from 'vitest'
import { getRoutePreviewStyle } from '@/features/widget-preview/widgets/route/style'
import { buildElevationPreviewStyle } from '@/features/widget-preview/widgets/elevation/style'

vi.mock('@/features/widget-preview/shared/textMeasurement', async () => {
  const actual = await vi.importActual('@/features/widget-preview/shared/textMeasurement')
  return {
    ...actual,
    getPreviewFontFamily: (fontFamily) => fontFamily || 'Arial',
  }
})

describe('plot preview style scaling', () => {
  test('route preview derives opacity and marker presentation', () => {
    const data = {
      width: 400,
      height: 200,
      remaining_line_width: 6,
      completed_line_width: 4,
      marker_size: 18,
      marker_variant_diameter: 44,
      marker_color: '#ffffff',
      marker_opacity: 100,
      remaining_line_color: '#ffffff',
      completed_line_color: '#ffffff',
      remaining_line_opacity: 35,
      completed_line_opacity: 100,
    }

    const result = { current: getRoutePreviewStyle(data, 2) }

    expect(result.current.remainingLineOpacity).toBe(0.35)
    expect(result.current.completedLineOpacity).toBe(1)
  })

  test('elevation preview derives opacity and scale presentation', () => {
    const data = {
      width: 400,
      height: 200,
      remaining_line_width: 6,
      completed_line_width: 4,
      remaining_line_opacity: 35,
      completed_line_opacity: 100,
      area_remaining_opacity: 12,
      area_completed_opacity: 24,
      marker_size: 16,
      marker_color: '#ffffff',
      marker_opacity: 100,
      show_elevation_metric: true,
      show_elevation_imperial: false,
      point_label: {
        font: 'Arial.ttf',
        font_size: 12,
        color: '#ffffff',
      },
    }

    const result = { current: buildElevationPreviewStyle(data, 2) }

    expect(result.current.remainingLineOpacity).toBe(0.35)
    expect(result.current.completedLineOpacity).toBe(1)
    expect(result.current.globalScale).toBe(2)
  })
})
