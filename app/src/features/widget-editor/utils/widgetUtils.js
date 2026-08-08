/**
 * Pure helper functions for widget creation, parsing, and geometry.
 * Domain constants used by these functions live in @/lib/standard-widgets.
 */

import { createFontSelection } from '@/lib/fonts'
import { initBackdropVariant } from '@/lib/widget/widget-resolver'
import { getDefaultFrameDimensions, getDisplayTypeConfigDefaults, getDisplayTypeDefaultFontSize } from '@/lib/widget/standard-metrics'
import {
  BACKDROP_DEFAULT_DISPLAY_TYPES,
  BACKDROP_CIRCLE_DEFAULTS,
  BACKDROP_RECTANGLE_DEFAULTS,
  TEXT_DEFAULTS,
  TEXT_FONT_SIZES,
  TEXT_LABEL_DEFAULTS,
  TYPE_DEFAULTS,
  HEADING_TAPE_DEFAULTS,
  LAP_TIMER_DEFAULTS,
  LAP_TIMER_MODES,
  COURSE_PLOT_DEFAULTS,
  ELEVATION_PLOT_DEFAULTS,
} from '@/lib/widget/standard-widgets'

const BACKDROP_SHARED_DEFAULT_KEYS = [
  'x',
  'y',
  'opacity',
  'fill_color',
  'fill_opacity',
  'border_thickness',
  'border_color',
  'border_opacity',
  'display_type',
]

const BACKDROP_DEFAULTS_BY_TYPE = {
  circle: BACKDROP_CIRCLE_DEFAULTS,
  rectangle: BACKDROP_RECTANGLE_DEFAULTS,
}

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
export function getGlobalColor(globalDefaults, key, fallback = '#ffffff') {
  return globalDefaults?.[key] || fallback
}

function createSharedMetricDefaults(type, globalDefaults, displayType, font) {
  return {
    ...TEXT_DEFAULTS,
    value: type,
    display_type: displayType,
    ...createFontSelection(font),
    font_size: getDisplayTypeDefaultFontSize(displayType) ?? TEXT_FONT_SIZES[type] ?? TEXT_FONT_SIZES.default,
    color: getGlobalColor(globalDefaults, 'color_values'),
    opacity: globalDefaults?.opacity ?? 1,
  }
}

/**
 * Returns course widget dimensions.
 *
 * @param {*} coursePoints - Value for course points.
 * @returns {object} Requested value or structure.
 */
export function getCourseWidgetDimensions(coursePoints) {
  const validPoints = (coursePoints || []).filter(([latitude, longitude]) => Number.isFinite(latitude) && Number.isFinite(longitude))

  if (validPoints.length < 2) {
    return null
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
    ...TEXT_LABEL_DEFAULTS,
    ...fontSelection,
    color: getGlobalColor(globalDefaults, 'color_text'),
    opacity: globalDefaults?.opacity ?? 1,
  }
}

/**
 * Creates backdrop defaults.
 *
 * @param {string} [displayType] - Optional display type override (e.g. "circle", "rectangle").
 * @returns {object} Backdrop data with shared fields at the top level and active geometry nested.
 */
export function createBackdropDefaults(displayType) {
  const resolvedType = displayType || BACKDROP_DEFAULT_DISPLAY_TYPES[0]
  const activeDefaults = BACKDROP_DEFAULTS_BY_TYPE[resolvedType]
  const seed = Object.fromEntries(BACKDROP_SHARED_DEFAULT_KEYS.map((key) => [key, activeDefaults[key]]))

  return initBackdropVariant(
    {
      ...seed,
      display_type: resolvedType,
    },
    resolvedType,
  )
}

/**
 * Creates metric value defaults.
 *
 * @param {*} type - Widget or value type identifier.
 * @param {*} globalDefaults - Value for global defaults.
 * @param {object} [selection] - Canonical widget creation selection.
 * @param {string} [selection.displayType] - Optional display type override (e.g. "text", "linear", "heading_tape").
 * @param {string} [selection.lapTimerMode] - Required lap timer mode for lap timer widgets.
 * @returns {object} Derived data structure for downstream use.
 */
export function createMetricValueDefaults(type, globalDefaults, selection = {}) {
  const { displayType, lapTimerMode } = selection
  const font = globalDefaults?.font_values || TEXT_DEFAULTS.font
  if (type === 'lap_timer') {
    const mode = LAP_TIMER_MODES.find((candidate) => candidate.value === lapTimerMode)
    if (!mode) throw new Error(`Unsupported lap timer mode: ${lapTimerMode}`)
    return {
      ...createSharedMetricDefaults(type, globalDefaults, 'lap_timer', font),
      ...LAP_TIMER_DEFAULTS,
      lap_timer_mode: mode.value,
      label: mode.label,
    }
  }

  const resolvedDisplayType = displayType || 'text'
  const sharedDefaults = createSharedMetricDefaults(type, globalDefaults, resolvedDisplayType, font)
  if (type === 'gradient') {
    return {
      ...sharedDefaults,
      ...TYPE_DEFAULTS.gradient,
      unit_color: getGlobalColor(globalDefaults, 'color_units'),
    }
  }

  // Build display_variants for boxed display types
  const displayVariants = {}
  if (type === 'heading' || resolvedDisplayType === 'heading_tape') {
    const frameDefaults = getDefaultFrameDimensions('heading_tape')
    displayVariants.heading_tape = {
      ...HEADING_TAPE_DEFAULTS,
      ...(frameDefaults || {}),
    }
  }
  if (resolvedDisplayType !== 'text' && !displayVariants[resolvedDisplayType]) {
    const variantDefaults = getDisplayTypeConfigDefaults(resolvedDisplayType)
    const frameDefaults = getDefaultFrameDimensions(resolvedDisplayType)
    if (variantDefaults || frameDefaults) {
      const seed = {
        ...(variantDefaults || {}),
        ...(frameDefaults || {}),
      }
      if (!seed.min_max_label_font && font) {
        seed.min_max_label_font = font
      }
      if (resolvedDisplayType === 'g_force') {
        seed.label_font = font
        seed.label_color = getGlobalColor(globalDefaults, 'color_values')
        seed.label_unit_color = getGlobalColor(globalDefaults, 'color_units')
      }
      displayVariants[resolvedDisplayType] = seed
    }
  }

  const result = {
    ...sharedDefaults,
    icon_color: getGlobalColor(globalDefaults, 'color_icons'),
    unit_color: getGlobalColor(globalDefaults, 'color_units'),
    ...TYPE_DEFAULTS[type],
  }

  if (Object.keys(displayVariants).length > 0) {
    result.display_variants = displayVariants
  }

  return result
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
  if (type === 'course') {
    // Required because course widget aspect ratio determined by the course points, so we can't set them as static defaults like other plot types
    const courseDimensions = getCourseWidgetDimensions(options.coursePoints)
    return {
      ...COURSE_PLOT_DEFAULTS,
      ...(courseDimensions ? { width: courseDimensions.width, height: courseDimensions.height } : {}),
      opacity: globalDefaults?.opacity ?? 1,
      color: getGlobalColor(globalDefaults, 'color_values'),
    }
  }

  const labelFont = globalDefaults?.font_values || 'Arial.ttf'
  return {
    ...ELEVATION_PLOT_DEFAULTS,
    opacity: globalDefaults?.opacity ?? 1,
    color: getGlobalColor(globalDefaults, 'color_values'),
    point_label: {
      ...createFontSelection(labelFont),
      font_size: options.sceneFontSize ?? 12.5,
      color: getGlobalColor(globalDefaults, 'color_values'),
    },
  }
}
