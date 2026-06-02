/**
 * Static constants for template serialization, normalization, and defaults.
 * Used by templateSnapshot.js for config normalization and file I/O.
 */

import { STANDARD_METRIC_WIDGET_TYPES } from '@/lib/standard-metrics'
import { TYPE_DEFAULTS, COURSE_PLOT_DEFAULTS, ELEVATION_PLOT_DEFAULTS } from '../../widget-editor/data/widgetDefaults'

/** File format identifier for OVRLEY template files. */
export const TEMPLATE_FILE_FORMAT = 'ovrley-template'

/** Current template file format version. */
export const TEMPLATE_FILE_VERSION = 2

/** Default export range when no custom range has been configured. */
export const DEFAULT_EXPORT_RANGE = {
  type: 'all',
  from: 0,
  to: 0,
  fromTime: '00:00:00',
  toTime: '00:00:00',
}

/** Keys preserved when normalizing a label widget. */
export const LABEL_KEYS = ['id', 'x', 'y', 'font', 'font_size', 'text', 'color', 'opacity']

/** Scene keys that are render-time-only artifacts, stripped during normalization. */
export const SCENE_RENDER_TIME_ONLY_KEYS = [
  'composite_video_path',
  'composite_bitrate',
  'composite_sync_offset',
  'composite_video_fps_num',
  'composite_video_fps_den',
  'composite_video_duration',
  'composite_render_duration',
  'composite_video_trim_start',
  'composite_widget_update_rate',
]

/** Keys shared by all metric value widgets during normalization. */
export const VALUE_SHARED_KEYS = [
  'id',
  'x',
  'y',
  'value',
  'font',
  'font_size',
  'color',
  'opacity',
  'prefix',
  'suffix',
  'decimals',
  'display_type',
  'display_variants',
]

/** Keys for the icon sub-object within a value widget. */
export const VALUE_ICON_KEYS = ['show_icon', 'icon_color', 'icon_size', 'icon_offset_x', 'icon_offset_y']

const STANDARD_METRIC_VALUE_KEYS = [...VALUE_ICON_KEYS, 'show_units', 'unit_color', 'display_unit', 'balance_format']

/** Per-type additional keys preserved when normalizing a value widget. */
export const VALUE_TYPE_KEYS = {
  ...Object.fromEntries(STANDARD_METRIC_WIDGET_TYPES.map((type) => [type, STANDARD_METRIC_VALUE_KEYS])),
  heading: [...VALUE_ICON_KEYS, 'show_units', 'unit_color', 'display_unit'],
  time: [...VALUE_ICON_KEYS, 'format'],
  gradient: ['unit_color', 'value_offset', 'triangle_positive_color', 'triangle_negative_color', 'show_sign', 'show_triangle', 'triangle_width'],
}

/** Keys preserved when normalizing a course plot widget. */
export const COURSE_PLOT_KEYS = [
  'id',
  'x',
  'y',
  'value',
  'width',
  'height',
  'opacity',
  'rotation',
  'completed_line_width',
  'remaining_line_width',
  'color',
  'completed_line_color',
  'completed_line_opacity',
  'remaining_line_color',
  'remaining_line_opacity',
  'simplify_tolerance_px',
  'target_density',
  'show_full_activity',
  'marker_variant',
  'marker_variant_diameter',
  'marker_size',
  'marker_color',
  'marker_opacity',
]

/** Keys preserved when normalizing an elevation plot widget (extends COURSE_PLOT_KEYS). */
export const ELEVATION_PLOT_KEYS = [
  ...COURSE_PLOT_KEYS,
  'area_completed_color',
  'area_completed_opacity',
  'area_remaining_color',
  'area_remaining_opacity',
  'show_elevation_metric',
  'show_elevation_imperial',
  'metric_label_offset_x',
  'metric_label_offset_y',
  'imperial_label_offset_x',
  'imperial_label_offset_y',
  'y_scale',
  'point_label',
]

/**
 * Allowed keys for display variant configs during normalization.
 * Keyed by display_type — each variant is normalized to its own allowed set.
 */
export const DISPLAY_VARIANT_KEYS = {
  heading_tape: [
    'width',
    'height',
    'rotation',
    'pixels_per_degree',
    'major_tick_interval',
    'minor_ticks_per_major',
    'show_major_ticks',
    'show_minor_ticks',
    'major_tick_length_pct',
    'minor_tick_length_pct',
    'major_tick_thickness',
    'minor_tick_thickness',
    'tick_color',
    'cardinal_tick_color',
    'tick_alignment',
    'show_minor_labels',
    'show_major_labels',
    'label_color',
    'cardinal_label_color',
    'label_font',
    'label_font_family',
    'label_font_size',
    'label_offset',
    'indicator_style',
    'indicator_placement',
    'show_indicator',
    'indicator_color',
    'indicator_size',
  ],
}

/** Default values applied to value widgets of each type during normalization. */
export const VALUE_DEFAULTS = Object.fromEntries(
  Object.entries(TYPE_DEFAULTS).map(([type, defaults]) => [type, type === 'gradient' ? defaults : { show_icon: true, ...defaults }]),
)

/** Default values applied to plot widgets of each type during normalization. */
export const PLOT_DEFAULTS = {
  course: { opacity: 1, rotation: 0, completed_line_width: 6, remaining_line_width: 6, ...COURSE_PLOT_DEFAULTS },
  elevation: {
    opacity: 1,
    rotation: 0,
    completed_line_width: 6,
    remaining_line_width: 6,
    area_remaining_color: '#00565c',
    ...ELEVATION_PLOT_DEFAULTS,
  },
}
