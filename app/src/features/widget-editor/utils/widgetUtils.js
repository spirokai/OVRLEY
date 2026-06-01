/**
 * Pure helper functions for widget creation, parsing, and geometry.
 * Domain constants used by these functions live in @/lib/widget-icons and ../data/widgetDefaults.
 */

import { createFontSelection } from '@/lib/fonts'
import {
  LABEL_DEFAULTS,
  SHARED_VALUE_DEFAULTS,
  FONT_SIZE_BY_TYPE,
  ICON_DEFAULTS,
  TYPE_DEFAULTS,
  PLOT_BASE_DEFAULTS,
  COURSE_PLOT_DEFAULTS,
  ELEVATION_PLOT_DEFAULTS,
  HEADING_DEFAULTS,
  COURSE_DIMENSIONS_FALLBACK,
} from '../data/widgetDefaults'

/**
 * Parses integer.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} fallback - Fallback value returned when input is invalid.
 * @returns {number} Result produced by the helper.
 */
export function parseInteger(value, fallback = 0) {
  const parsed = parseInt(value, 10)
  return Number.isFinite(parsed) ? parsed : fallback
}

/**
 * Constrains a value to the provided minimum and maximum bounds.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} min - Lower bound used by the calculation.
 * @param {*} max - Upper bound used by the calculation.
 * @returns {number} Result produced by the helper.
 */
/**
 * Returns widget font.
 *
 * @param {*} widget - Widget definition being rendered or edited.
 * @param {*} fallback - Fallback value returned when input is invalid.
 * @returns {*} Requested value or structure.
 */
export function getWidgetFont(widget, fallback = 'Arial.ttf') {
  return widget.data.font || widget.data.font_family || fallback
}

/**
 * Returns global color.
 *
 * @param {*} globalDefaults - Value for global defaults.
 * @param {*} key - Lookup key for the requested value.
 * @param {*} fallback - Fallback value returned when input is invalid.
 * @returns {*} Requested value or structure.
 */
export function getGlobalColor(globalDefaults, key, fallback) {
  return globalDefaults?.[key] || fallback
}

/**
 * Returns course widget dimensions.
 *
 * @param {*} coursePoints - Value for course points.
 * @returns {object} Requested value or structure.
 */
function getCourseWidgetDimensions(coursePoints) {
  const validPoints = (coursePoints || []).filter(([latitude, longitude]) => Number.isFinite(latitude) && Number.isFinite(longitude))

  if (validPoints.length < 2) {
    return COURSE_DIMENSIONS_FALLBACK
  }

  const meanLatitudeRadians = (validPoints.reduce((sum, [latitude]) => sum + latitude, 0) / validPoints.length) * (Math.PI / 180)
  const projectedX = validPoints.map(([, longitude]) => longitude * Math.cos(meanLatitudeRadians))
  const projectedY = validPoints.map(([latitude]) => latitude)
  const spanX = Math.max(Math.max(...projectedX) - Math.min(...projectedX), 1e-6)
  const spanY = Math.max(Math.max(...projectedY) - Math.min(...projectedY), 1e-6)

  if (spanX >= spanY) {
    return {
      width: 400,
      height: Math.max(Math.round((400 * spanY) / spanX), 80),
    }
  }

  return {
    width: Math.max(Math.round((400 * spanX) / spanY), 80),
    height: 400,
  }
}

/**
 * Creates label defaults.
 *
 * @param {*} globalDefaults - Value for global defaults.
 * @returns {object} Derived data structure for downstream use.
 */
export function createLabelDefaults(globalDefaults) {
  const font = globalDefaults?.font_text || 'Arial.ttf'
  const fontSelection = createFontSelection(font)
  return {
    ...LABEL_DEFAULTS,
    ...fontSelection,
    color: getGlobalColor(globalDefaults, 'color_text'),
    opacity: globalDefaults?.opacity ?? 1,
  }
}

/**
 * Creates metric value defaults.
 *
 * @param {*} type - Widget or value type identifier.
 * @param {*} globalDefaults - Value for global defaults.
 * @returns {object} Derived data structure for downstream use.
 */
export function createMetricValueDefaults(type, globalDefaults) {
  const font = globalDefaults?.font_values || 'Furore.otf'
  const fontSelection = createFontSelection(font)
  const sharedDefaults = {
    ...SHARED_VALUE_DEFAULTS,
    value: type,
    ...fontSelection,
    font_size: FONT_SIZE_BY_TYPE[type] || FONT_SIZE_BY_TYPE.default,
    color: getGlobalColor(globalDefaults, 'color_values'),
    opacity: globalDefaults?.opacity ?? 1,
  }

  if (type === 'gradient') {
    return {
      ...sharedDefaults,
      ...TYPE_DEFAULTS.gradient,
      unit_color: getGlobalColor(globalDefaults, 'color_units', '#ffffff'),
    }
  }

  return {
    ...sharedDefaults,
    ...ICON_DEFAULTS,
    icon_color: getGlobalColor(globalDefaults, 'color_icons'),
    unit_color: getGlobalColor(globalDefaults, 'color_units', '#ffffff'),
    ...TYPE_DEFAULTS[type],
  }
}

/**
 * Creates plot defaults.
 *
 * @param {*} type - Widget or value type identifier.
 * @param {*} globalDefaults - Value for global defaults.
 * @param {*} options - Configuration options for the helper.
 * @returns {object} Derived data structure for downstream use.
 */
export function createPlotDefaults(type, globalDefaults, options = {}) {
  const courseDimensions = type === 'course' ? getCourseWidgetDimensions(options.coursePoints) : COURSE_DIMENSIONS_FALLBACK
  const base = {
    ...PLOT_BASE_DEFAULTS,
    value: type,
    width: courseDimensions.width,
    height: courseDimensions.height,
    opacity: globalDefaults?.opacity ?? 1,
  }

  if (type === 'course') {
    return {
      ...base,
      ...COURSE_PLOT_DEFAULTS,
      color: getGlobalColor(globalDefaults, 'color_values'),
    }
  }

  const labelFont = globalDefaults?.font_values || 'Furore.otf'
  return {
    ...base,
    ...ELEVATION_PLOT_DEFAULTS,
    color: getGlobalColor(globalDefaults, 'color_values'),
    point_label: {
      ...createFontSelection(labelFont),
      font_size: options.sceneFontSize ?? 12.5,
      color: getGlobalColor(globalDefaults, 'color_values'),
    },
  }
}

/**
 * Creates heading compass tape widget defaults.
 *
 * @param {*} globalDefaults - Value for global defaults.
 * @returns {object} Derived data structure for downstream use.
 */
export function createHeadingDefaults(globalDefaults) {
  const base = createMetricValueDefaults('heading', globalDefaults)
  const headingFont = globalDefaults?.font_values || base.label_font || 'Arial.ttf'
  const headingFontSelection = createFontSelection(headingFont)

  return {
    ...base,
    label_font: headingFontSelection.font,
    label_font_family: headingFontSelection.font_family,
  }
}
