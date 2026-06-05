import { renderHook } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { useRoutePreviewStyle } from '@/features/widget-preview/hooks/useRoutePreviewStyle'
import { useElevationPreviewStyle } from '@/features/widget-preview/hooks/useElevationPreviewStyle'

vi.mock('@/features/widget-preview/hooks/useFontMetricsVersion', () => ({
  useFontMetricsVersion: () => 0,
}))

vi.mock('@/features/widget-preview/utils/textMeasurement', async () => {
  const actual = await vi.importActual('@/features/widget-preview/utils/textMeasurement')
  return {
    ...actual,
    getPreviewFontFamily: (fontFamily) => fontFamily || 'Arial',
  }
})

describe('plot preview style scaling', () => {
  test('route preview keeps configured stroke widths even when global scale is applied externally', () => {
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

    const { result } = renderHook(() => useRoutePreviewStyle(data, 2))

    expect(result.current.remainingLineWidth).toBe(6)
    expect(result.current.completedLineWidth).toBe(4)
  })

  test('elevation preview keeps configured stroke widths even when global scale is applied externally', () => {
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

    const { result } = renderHook(() => useElevationPreviewStyle(data, 2))

    expect(result.current.remainingLineWidth).toBe(6)
    expect(result.current.completedLineWidth).toBe(4)
    expect(result.current.safeGlobalScale).toBe(2)
  })
})
