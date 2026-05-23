/**
 * Static default value objects for widget creation.
 * Used as base templates by the factory functions in ../utils/widgetUtils.js.
 * Only contains static constants — no function calls or side effects.
 */

/** Default position and label fields for a text/label widget. */
export const LABEL_DEFAULTS = {
  x: 100,
  y: 100,
  font_size: 60,
  text: 'New Text',
}

/** Default position and formatting fields shared by metric value widgets. */
export const SHARED_VALUE_DEFAULTS = {
  x: 100,
  y: 100,
  prefix: '',
  suffix: '',
  decimals: 0,
}

/** Default font sizes per metric type. Falls back to 100 for unlisted types. */
export const FONT_SIZE_BY_TYPE = {
  time: 72,
  gradient: 96,
  default: 100,
}

/** Default icon fields shared by metric value widgets that show an icon. */
export const ICON_DEFAULTS = {
  show_icon: true,
  icon_size: 28,
  icon_offset_x: 0,
  icon_offset_y: 0,
}

/** Type-specific default fields for each metric value widget type (dynamic fields omitted). */
export const TYPE_DEFAULTS = {
  speed: { show_units: true, speed_unit: 'kmh', unit_color: '#ffffff' },
  temperature: { show_units: true, temperature_unit: 'celsius', unit_color: '#ffffff' },
  heartrate: { show_units: false, unit_color: '#ffffff' },
  cadence: { show_units: false, unit_color: '#ffffff' },
  power: { show_units: false, unit_color: '#ffffff' },
  time: { format: 'time-24' },
  gradient: {
    decimals: 0,
    unit_color: '#ffffff',
    value_offset: 0,
    show_sign: true,
    show_triangle: true,
    triangle_width: 72,
    triangle_positive_color: '#40e0d0',
    triangle_negative_color: '#c65102',
  },
}

/** Default position and geometry fields for a plot (course/elevation) widget. */
export const PLOT_BASE_DEFAULTS = {
  x: 100,
  y: 100,
  rotation: 0,
  completed_line_width: 6,
  remaining_line_width: 6,
}

/** Default line/marker fields specific to course plot widgets. */
export const COURSE_PLOT_DEFAULTS = {
  completed_line_opacity: 100,
  remaining_line_opacity: 35,
  simplify_tolerance_px: 1,
  target_density: 1,
  show_full_activity: false,
  marker_size: 18,
  marker_opacity: 100,
  completed_line_color: '#ffffff',
  remaining_line_color: '#ffffff',
  marker_color: '#ffffff',
}

/** Default line/area/marker fields specific to elevation plot widgets. */
export const ELEVATION_PLOT_DEFAULTS = {
  completed_line_opacity: 100,
  remaining_line_opacity: 35,
  area_completed_opacity: 24,
  area_remaining_opacity: 12,
  marker_size: 16,
  marker_opacity: 100,
  show_elevation_metric: true,
  show_elevation_imperial: false,
  show_full_activity: false,
  y_scale: 1,
  simplify_tolerance_px: 1,
  target_density: 0.75,
  metric_label_offset_x: 0,
  metric_label_offset_y: 0,
  imperial_label_offset_x: 0,
  imperial_label_offset_y: 0,
  completed_line_color: '#ffffff',
  remaining_line_color: '#ffffff',
  area_completed_color: '#ffffff',
  area_remaining_color: '#ffffff',
  marker_color: '#ffffff',
}

/** Fallback dimensions when course points data is unavailable. */
export const COURSE_DIMENSIONS_FALLBACK = { width: 400, height: 200 }
