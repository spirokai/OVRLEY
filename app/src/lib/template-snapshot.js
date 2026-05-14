/**
 * Provides shared template snapshot utilities for the app.
 */

import { normalizeColorFields } from './color-utils'
import { DEFAULT_GLOBAL_DEFAULTS, GLOBAL_DEFAULT_KEYS, SCENE_DERIVED_SETTING_KEYS, SCENE_GLOBAL_DEFAULT_KEYS } from './config-utils'

export { DEFAULT_GLOBAL_DEFAULTS } from './config-utils'

export const TEMPLATE_FILE_FORMAT = 'ovrley-template'
export const TEMPLATE_FILE_VERSION = 1

export const DEFAULT_EXPORT_RANGE = {
  type: 'all',
  from: 0,
  to: 0,
  fromTime: '00:00:00',
  toTime: '00:00:00',
}

const LABEL_KEYS = ['x', 'y', 'font', 'font_size', 'text', 'color', 'opacity']

const SCENE_RENDER_TIME_ONLY_KEYS = [
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

const VALUE_SHARED_KEYS = ['x', 'y', 'value', 'font', 'font_size', 'color', 'opacity', 'prefix', 'suffix', 'decimals']

const VALUE_ICON_KEYS = ['show_icon', 'icon_color', 'icon_size', 'icon_offset_x', 'icon_offset_y']

const VALUE_TYPE_KEYS = {
  speed: [...VALUE_ICON_KEYS, 'show_units', 'speed_unit'],
  heartrate: [...VALUE_ICON_KEYS, 'show_units'],
  cadence: [...VALUE_ICON_KEYS, 'show_units'],
  power: [...VALUE_ICON_KEYS, 'show_units'],
  temperature: [...VALUE_ICON_KEYS, 'show_units', 'temperature_unit'],
  time: [...VALUE_ICON_KEYS, 'format'],
  gradient: ['value_offset', 'triangle_positive_color', 'triangle_negative_color', 'show_sign', 'show_triangle', 'triangle_width'],
}

const COURSE_PLOT_KEYS = [
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

const ELEVATION_PLOT_KEYS = [
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

const VALUE_DEFAULTS = {
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

const PLOT_DEFAULTS = {
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

/**
 * Handles clone serializable.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Result produced by the helper.
 */
function cloneSerializable(value) {
  if (value === undefined) return undefined
  return JSON.parse(JSON.stringify(value))
}

function pickDefined(source, keys) {
  return keys.reduce((result, key) => {
    if (source?.[key] !== undefined) {
      result[key] = source[key]
    }
    return result
  }, {})
}

function normalizeGlobalDefaults(globalDefaults) {
  return normalizeColorFields({
    ...DEFAULT_GLOBAL_DEFAULTS,
    ...pickDefined(cloneSerializable(globalDefaults) || {}, GLOBAL_DEFAULT_KEYS),
  })
}

function mergeSceneGlobalDefaults(scene, globalDefaults) {
  const sceneDefaults = {}
  SCENE_GLOBAL_DEFAULT_KEYS.forEach((key) => {
    if (scene?.[key] !== undefined) {
      sceneDefaults[key] = scene[key]
    }
  })

  return {
    ...normalizeGlobalDefaults({
      ...sceneDefaults,
      ...(cloneSerializable(globalDefaults) || {}),
    }),
  }
}

function normalizeScene(scene = {}) {
  const nextScene = cloneSerializable(scene) || {}

  SCENE_DERIVED_SETTING_KEYS.forEach((key) => {
    delete nextScene[key]
  })
  SCENE_RENDER_TIME_ONLY_KEYS.forEach((key) => {
    delete nextScene[key]
  })

  return normalizeColorFields(nextScene)
}

function normalizeLabel(label = {}) {
  return normalizeColorFields(pickDefined(label, LABEL_KEYS))
}

function normalizeValue(value = {}) {
  const type = value.value
  const withDefaults = {
    ...VALUE_DEFAULTS[type],
    ...value,
  }
  const keys = [...VALUE_SHARED_KEYS, ...(VALUE_TYPE_KEYS[type] || VALUE_ICON_KEYS)]

  return normalizeColorFields(pickDefined(withDefaults, keys))
}

function normalizePointLabel(pointLabel, config, globalDefaults) {
  const fallbackFont = globalDefaults?.font_values || config?.scene?.font
  const fallbackColor = pointLabel?.color || globalDefaults?.color_values || '#ffffff'

  return normalizeColorFields({
    ...(fallbackFont ? { font: fallbackFont } : {}),
    font_size: pointLabel?.font_size ?? config?.scene?.font_size ?? 12.5,
    color: fallbackColor,
    ...pickDefined(pointLabel, ['font', 'font_size', 'color']),
  })
}

function normalizePlot(plot = {}, config, globalDefaults) {
  const type = plot.value
  const withDefaults = {
    ...PLOT_DEFAULTS[type],
    ...plot,
  }

  if (type === 'elevation') {
    withDefaults.point_label = normalizePointLabel(plot.point_label, config, globalDefaults)
  }

  const keys = type === 'elevation' ? ELEVATION_PLOT_KEYS : COURSE_PLOT_KEYS
  return normalizeColorFields(pickDefined(withDefaults, keys))
}

export function normalizeTemplateConfig(config, globalDefaults) {
  const nextConfig = cloneSerializable(config) || {}
  const normalizedConfig = {
    scene: normalizeScene(nextConfig.scene),
    labels: Array.isArray(nextConfig.labels) ? nextConfig.labels.map(normalizeLabel) : [],
    values: Array.isArray(nextConfig.values) ? nextConfig.values.map(normalizeValue) : [],
    plots: Array.isArray(nextConfig.plots) ? nextConfig.plots.map((plot) => normalizePlot(plot, nextConfig, globalDefaults)) : [],
  }

  return normalizedConfig
}

/**
 * Handles sanitize template filename.
 *
 * @param {*} name - Value for name.
 * @returns {*} Result produced by the helper.
 */
export function sanitizeTemplateFilename(name) {
  const normalized = String(name || 'ovrley_template')
    .trim()
    .replace(/\.[^/.]+$/, '')
    .replace(/[^a-zA-Z0-9._-]+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '')

  return `${normalized || 'ovrley_template'}.json`
}

/**
 * Creates template state.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.config - Overlay template configuration data.
 * @param {*} options.globalDefaults - Value for global defaults.
 * @returns {object} Derived data structure for downstream use.
 */
export function createTemplateState({ config, globalDefaults }) {
  const nextGlobalDefaults = mergeSceneGlobalDefaults(config?.scene, globalDefaults)

  return {
    config: normalizeTemplateConfig(config, nextGlobalDefaults),
    settings: {
      globalDefaults: nextGlobalDefaults,
    },
  }
}

/**
 * Creates template file payload.
 *
 * @param {*} state - Value for state.
 * @param {*} meta - Value for meta.
 * @returns {object} Derived data structure for downstream use.
 */
export function createTemplateFilePayload(state, meta = {}) {
  return {
    format: TEMPLATE_FILE_FORMAT,
    version: TEMPLATE_FILE_VERSION,
    name: meta.name || null,
    savedAt: new Date().toISOString(),
    ...createTemplateState(state),
  }
}

/**
 * Normalizes template file payload.
 *
 * @param {*} rawTemplate - Value for raw template.
 * @param {*} fallbackState - Value for fallback state.
 * @returns {object} Derived data structure for downstream use.
 */
export function normalizeTemplateFilePayload(rawTemplate, _fallbackState = {}) {
  if (!rawTemplate || typeof rawTemplate !== 'object') {
    throw new Error('Template file is empty or invalid.')
  }

  if (rawTemplate.format === TEMPLATE_FILE_FORMAT && rawTemplate.config && rawTemplate.settings) {
    const nextGlobalDefaults = mergeSceneGlobalDefaults(rawTemplate.config.scene, rawTemplate.settings.globalDefaults)

    return {
      ...createTemplateState({
        config: rawTemplate.config,
        globalDefaults: nextGlobalDefaults,
      }),
      name: rawTemplate.name || null,
    }
  }

  throw new Error('Unsupported template file format.')
}

/**
 * Handles template states equal.
 *
 * @param {*} left - Left-hand comparison value.
 * @param {*} right - Right-hand comparison value.
 * @returns {*} Result produced by the helper.
 */
export function templateStatesEqual(left, right) {
  return JSON.stringify(left) === JSON.stringify(right)
}

/**
 * Handles stringify template file.
 *
 * @param {*} payload - Structured payload produced by the helper.
 * @returns {*} Result produced by the helper.
 */
export function stringifyTemplateFile(payload) {
  return JSON.stringify(payload, null, 2)
}

/**
 * Handles download template file.
 *
 * @param {*} payload - Structured payload produced by the helper.
 * @param {*} filename - Target filename for the operation.
 * @returns {*} Result produced by the helper.
 */
export function downloadTemplateFile(payload, filename) {
  const blob = new Blob([stringifyTemplateFile(payload)], {
    type: 'application/json',
  })
  const objectUrl = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = objectUrl
  anchor.download = sanitizeTemplateFilename(filename)
  document.body.appendChild(anchor)
  anchor.click()
  document.body.removeChild(anchor)
  URL.revokeObjectURL(objectUrl)
}
