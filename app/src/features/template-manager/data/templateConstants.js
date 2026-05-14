/**
 * Static constants for template serialization, normalization, and defaults.
 * Used by templateSnapshot.js for config normalization and file I/O.
 */

/** File format identifier for OVRLEY template files. */
export const TEMPLATE_FILE_FORMAT = 'ovrley-template'

/** Current template file format version. */
export const TEMPLATE_FILE_VERSION = 1

/** Default export range when no custom range has been configured. */
export const DEFAULT_EXPORT_RANGE = {
  type: 'all',
  from: 0,
  to: 0,
  fromTime: '00:00:00',
  toTime: '00:00:00',
}

/** Keys preserved when normalizing a label widget. */
export const LABEL_KEYS = ['x', 'y', 'font', 'font_size', 'text', 'color', 'opacity']

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
export const VALUE_SHARED_KEYS = ['x', 'y', 'value', 'font', 'font_size', 'color', 'opacity', 'prefix', 'suffix', 'decimals']

/** Keys for the icon sub-object within a value widget. */
export const VALUE_ICON_KEYS = ['show_icon', 'icon_color', 'icon_size', 'icon_offset_x', 'icon_offset_y']

/** Per-type additional keys preserved when normalizing a value widget. */
export const VALUE_TYPE_KEYS = {
  speed: [...VALUE_ICON_KEYS, 'show_units', 'speed_unit'],
  heartrate: [...VALUE_ICON_KEYS, 'show_units'],
  cadence: [...VALUE_ICON_KEYS, 'show_units'],
  power: [...VALUE_ICON_KEYS, 'show_units'],
  temperature: [...VALUE_ICON_KEYS, 'show_units', 'temperature_unit'],
  time: [...VALUE_ICON_KEYS, 'format'],
  gradient: ['value_offset', 'triangle_positive_color', 'triangle_negative_color', 'show_sign', 'show_triangle', 'triangle_width'],
}

/** Keys preserved when normalizing a course plot widget. */
export const COURSE_PLOT_KEYS = [
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

/** Default values applied to value widgets of each type during normalization. */
export const VALUE_DEFAULTS = {
  speed: {
    show_icon: true,
    show_units: true,
    speed_unit: 'kmh',
  },
  heartrate: {
    show_icon: true,
    show_units: false,
  },
  cadence: {
    show_icon: true,
    show_units: false,
  },
  power: {
    show_icon: true,
    show_units: false,
  },
  temperature: {
    show_icon: true,
    show_units: true,
    temperature_unit: 'celsius',
  },
  time: {
    show_icon: true,
    show_units: false,
    format: 'time-24',
  },
  gradient: {
    decimals: 0,
    value_offset: 0,
    triangle_positive_color: '#40e0d0',
    triangle_negative_color: '#c65102',
    show_sign: true,
    show_triangle: true,
    triangle_width: 72,
  },
}

/** Default values applied to plot widgets of each type during normalization. */
export const PLOT_DEFAULTS = {
  course: {
    opacity: 1,
    rotation: 0,
    completed_line_width: 6,
    remaining_line_width: 6,
    completed_line_opacity: 100,
    remaining_line_opacity: 35,
    simplify_tolerance_px: 1,
    target_density: 1,
    show_full_activity: false,
    marker_size: 18,
    marker_opacity: 100,
  },
  elevation: {
    opacity: 1,
    rotation: 0,
    completed_line_width: 6,
    remaining_line_width: 6,
    completed_line_opacity: 100,
    remaining_line_opacity: 35,
    area_completed_opacity: 24,
    area_completed_color: '#ffffff',
    area_remaining_color: '#00565c',
    area_remaining_opacity: 12,
    show_elevation_metric: true,
    show_elevation_imperial: false,
    metric_label_offset_x: 0,
    metric_label_offset_y: 0,
    imperial_label_offset_x: 0,
    imperial_label_offset_y: 0,
    y_scale: 1,
    simplify_tolerance_px: 1,
    target_density: 0.75,
    show_full_activity: false,
    marker_size: 16,
    marker_opacity: 100,
  },
}
