import { describe, expect, test } from 'vitest'
import { translateBackendError } from '@/lib/utils'

describe('backend error messages for display', () => {
  test('translates tagged image errors', () => {
    expect(translateBackendError('Raster widget-1 has no selected image. [raster_error:no_image:widget-1]')).toBe(
      'Image widget-1 has no selected file. Select one before rendering.',
    )
    expect(translateBackendError('Raster widget-1 is missing. [raster_error:missing:widget-1]')).toBe(
      'widget-1: Image file is missing. Select another image.',
    )
    expect(translateBackendError('Invalid configuration [raster_error:invalid_config:configuration]')).toBe('Image settings are invalid.')
  })

  test('preserves unrelated messages', () => {
    expect(translateBackendError('Output already exists')).toBe('Output already exists')
  })
})
