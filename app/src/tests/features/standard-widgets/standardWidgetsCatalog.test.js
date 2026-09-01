import { describe, expect, test } from 'vitest'

import {
  BACKDROP_CIRCLE_DEFAULTS,
  BACKDROP_DEFAULT_DISPLAY_TYPES,
  BACKDROP_RECTANGLE_DEFAULTS,
  BACKDROP_TYPE_DEFINITIONS,
  BACKDROP_TYPE_LABEL_KEYS,
  COURSE_PLOT_DEFAULTS,
  ELEVATION_PLOT_DEFAULTS,
  GRADIENT_DEFAULTS,
  TEXT_LABEL_DEFAULTS,
  WIDGET_TYPE_DEFINITIONS,
  getBackdropTypeOptions,
} from '@/lib/widget/standard-widgets'
import i18next from '@/i18n'

describe('standard widget manifest contract', () => {
  test('discovers non-metric widget definitions from their manifest-owned type metadata', () => {
    expect(Object.keys(WIDGET_TYPE_DEFINITIONS)).toEqual(expect.arrayContaining(['backdrop', 'label', 'time', 'elevation', 'course', 'gradient']))
    expect(WIDGET_TYPE_DEFINITIONS.course).toMatchObject({
      type: 'course',
      nameKey: 'widgets.types.course.name',
      shortNameKey: 'widgets.types.course.shortName',
      category: 'general',
    })
  })

  test('preserves existing defaults through definition-backed exports', () => {
    expect(COURSE_PLOT_DEFAULTS).toMatchObject({
      value: 'course',
      width: 400,
      height: 200,
      completed_line_opacity: 100,
    })
    expect(ELEVATION_PLOT_DEFAULTS.point_label).toEqual({
      font: 'Arial.ttf',
      font_size: 12.5,
      color: '#ffffff',
    })
    expect(GRADIENT_DEFAULTS.triangle_width).toBe(72)
    expect(TEXT_LABEL_DEFAULTS.text).toBe('New Text')
  })

  test('exposes backdrop definitions and rectangle as the default display type', () => {
    expect(Object.keys(BACKDROP_TYPE_DEFINITIONS)).toEqual(['circle', 'rectangle'])
    expect(BACKDROP_TYPE_LABEL_KEYS).toEqual({
      circle: 'widgets.backdropTypes.circle',
      rectangle: 'widgets.backdropTypes.rectangle',
    })
    expect(BACKDROP_DEFAULT_DISPLAY_TYPES).toEqual(['rectangle'])
    expect(BACKDROP_CIRCLE_DEFAULTS).toEqual({
      display_type: 'circle',
      x: 100,
      y: 100,
      opacity: 1,
      diameter: 200,
      fill_color: '#212121',
      fill_opacity: 0.5,
      border_thickness: 0,
      border_color: '#D6D6D6',
      border_opacity: 1,
    })
    expect(BACKDROP_RECTANGLE_DEFAULTS).toEqual({
      display_type: 'rectangle',
      x: 100,
      y: 100,
      opacity: 1,
      width: 300,
      height: 150,
      fill_color: '#212121',
      fill_opacity: 0.5,
      border_thickness: 0,
      border_color: '#D6D6D6',
      border_opacity: 1,
      corner_radius: 20,
      round_top_left: true,
      round_top_right: false,
      round_bottom_left: false,
      round_bottom_right: true,
    })
    expect(getBackdropTypeOptions(i18next.t)).toEqual([
      { value: 'circle', label: 'Circle' },
      { value: 'rectangle', label: 'Rectangle' },
    ])
  })
})
