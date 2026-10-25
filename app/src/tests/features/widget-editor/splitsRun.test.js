import {
  buildSplitBarHeightUpdate,
  buildSplitDistanceUpdate,
  buildVisibleColumnsUpdate,
  maxSplitBarBorderThickness,
} from '@/features/widget-editor/utils/splitsRun'
import { createWidgetDefaults } from '@/lib/widget/widget-defaults'

test('splits widgets start from the manifest defaults with global label typography', () => {
  const data = createWidgetDefaults({ type: 'splits_run' }, { font_text: 'Teko.ttf', color_text: '#123456', font_values: 'Arial.ttf' })
  expect(data).toMatchObject({
    display_type: 'splits_run',
    label: 'SPLITS',
    label_font: 'Teko.ttf',
    label_color: '#123456',
    split_distance: 1,
    split_unit: 'km',
    max_splits: 10,
    visible_columns: ['split', 'bar', 'pace'],
    bar_width: 160,
    track_border_thickness: 0,
  })
})

test('column selection keeps drawn order and never clears the last column', () => {
  expect(buildVisibleColumnsUpdate(['pace', 'split'])).toEqual({ visible_columns: ['split', 'pace'] })
  expect(buildVisibleColumnsUpdate([])).toBeNull()
})

test('split length accepts only positive distances', () => {
  expect(buildSplitDistanceUpdate('0.5')).toEqual({ split_distance: 0.5 })
  expect(buildSplitDistanceUpdate('0')).toBeNull()
  expect(buildSplitDistanceUpdate('')).toBeNull()
  expect(buildSplitDistanceUpdate('abc')).toBeNull()
})

test('a lower bar keeps its border and corners inside the new height', () => {
  const data = { track_thickness: 20, track_corner_radius: 8, track_border_thickness: 6 }
  const update = buildSplitBarHeightUpdate(data, 6)
  expect(update).toEqual({ track_thickness: 6, track_corner_radius: 3, track_border_thickness: 2 })
  expect(update.track_border_thickness * 2).toBeLessThan(update.track_thickness)
  expect(maxSplitBarBorderThickness({ track_thickness: 1 })).toBe(0)
  expect(maxSplitBarBorderThickness({ track_thickness: 40 })).toBe(6)
})
