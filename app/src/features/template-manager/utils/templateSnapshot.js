/**
 * Template snapshot utilities — config normalization, file serialization,
 * and state comparison for OVRLEY template files.
 * Static constants live in ../data/templateConstants.js.
 */

import { normalizeColorFields } from '@/lib/color-utils'
import { DEFAULT_GLOBAL_DEFAULTS, GLOBAL_DEFAULT_KEYS, SCENE_DERIVED_SETTING_KEYS, SCENE_GLOBAL_DEFAULT_KEYS } from '@/lib/config-utils'
import {
  TEMPLATE_FILE_FORMAT,
  TEMPLATE_FILE_VERSION,
  LABEL_KEYS,
  SCENE_RENDER_TIME_ONLY_KEYS,
  VALUE_SHARED_KEYS,
  VALUE_ICON_KEYS,
  VALUE_TYPE_KEYS,
  COURSE_PLOT_KEYS,
  ELEVATION_PLOT_KEYS,
  VALUE_DEFAULTS,
  PLOT_DEFAULTS,
} from '../data/templateConstants'

export { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/config-utils'

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
