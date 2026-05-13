/**
 * Provides store utilities related to store utils.
 */

import { DEFAULT_EXPORT_RANGE, normalizeTemplateConfig } from '../lib/template-snapshot'
import { normalizeColorFields } from '../lib/color-utils'
import { DEFAULT_GLOBAL_DEFAULTS } from '../lib/config-utils'

let isUpdatingFromConfig = false

export const DEFAULT_CONFIG = {
  scene: {
    width: 1920,
    height: 1080,
    fps: 30,
    start: 0,
    end: 60,
    font: 'Arial.ttf',
    color: '#ffffff',
    font_size: 30,
  },
  labels: [],
  values: [],
  plots: [],
}

export const DEFAULT_RENDER_PROGRESS = {
  renderId: null,
  current: 0,
  total: 0,
  percent: 0,
  status: 'idle',
  message: '',
  estimatedSecondsRemaining: null,
  encoded: 0,
  filename: null,
}

/**
 * Reads stored json.
 *
 * @param {*} key - Lookup key for the requested value.
 * @param {*} fallback - Fallback value returned when input is invalid.
 * @returns {*} Requested value or structure.
 */
export function readStoredJson(key, fallback) {
  const value = localStorage.getItem(key)
  if (!value) return fallback

  try {
    return JSON.parse(value)
  } catch {
    return fallback
  }
}

/**
 * Reads stored int.
 *
 * @param {*} key - Lookup key for the requested value.
 * @param {*} fallback - Fallback value returned when input is invalid.
 * @returns {*} Requested value or structure.
 */
export function readStoredInt(key, fallback) {
  const value = parseInt(localStorage.getItem(key) || `${fallback}`, 10)
  return Number.isFinite(value) ? value : fallback
}

/**
 * Handles persist serializable.
 *
 * @param {*} key - Lookup key for the requested value.
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Result produced by the helper.
 */
export function persistSerializable(key, value) {
  if (value === null || value === undefined) {
    localStorage.removeItem(key)
    return
  }

  localStorage.setItem(key, JSON.stringify(value))
}

/**
 * Handles clone serializable.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Result produced by the helper.
 */
export function cloneSerializable(value) {
  return JSON.parse(JSON.stringify(value))
}

/**
 * Checks whether has serializable changed.
 *
 * @param {*} left - Left-hand comparison value.
 * @param {*} right - Right-hand comparison value.
 * @returns {boolean} Whether the condition is satisfied.
 */
export function hasSerializableChanged(left, right) {
  return JSON.stringify(left) !== JSON.stringify(right)
}

/**
 * Updates config persistence.
 *
 * @param {*} state - Value for state.
 * @returns {*} Result produced by the helper.
 */
export function updateConfigPersistence(state) {
  localStorage.setItem('editorConfig', JSON.stringify(state.config))

  if (state.lastRenderedConfig) {
    state.hasUnrenderedChanges = hasSerializableChanged(state.config, state.lastRenderedConfig)
    return
  }

  state.hasUnrenderedChanges = true
}

/**
 * Reads stored config.
 * @returns {*} Requested value or structure.
 */
export function readStoredConfig() {
  const savedConfig = localStorage.getItem('editorConfig')
  if (savedConfig) {
    try {
      const parsed = JSON.parse(savedConfig)
      if (parsed && parsed.scene) {
        const globalDefaults = {
          ...DEFAULT_GLOBAL_DEFAULTS,
          ...normalizeColorFields(readStoredJson('globalDefaults', {}) || {}),
        }
        return normalizeTemplateConfig(parsed, globalDefaults)
      }
    } catch {
      console.warn('Failed to parse saved config, using default')
    }
  }

  return DEFAULT_CONFIG
}

/**
 * Handles begin config update.
 * @returns {*} Result produced by the helper.
 */
export function beginConfigUpdate() {
  const wasUpdating = isUpdatingFromConfig
  isUpdatingFromConfig = true
  return wasUpdating
}

/**
 * Handles end config update soon.
 * @returns {*} Result produced by the helper.
 */
export function endConfigUpdateSoon() {
  setTimeout(() => {
    isUpdatingFromConfig = false
  }, 100)
}

/**
 * Checks whether is config update in progress.
 * @returns {boolean} Whether the condition is satisfied.
 */
export function isConfigUpdateInProgress() {
  return isUpdatingFromConfig
}

/**
 * Reads stored template settings.
 * @returns {object} Requested value or structure.
 */
export function readStoredTemplateSettings() {
  const storedGlobalDefaults = normalizeColorFields(readStoredJson('globalDefaults', {}) || {})
  const globalDefaults = Object.keys(DEFAULT_GLOBAL_DEFAULTS).reduce(
    (result, key) => ({
      ...result,
      [key]: storedGlobalDefaults[key] === undefined ? DEFAULT_GLOBAL_DEFAULTS[key] : storedGlobalDefaults[key],
    }),
    {},
  )

  return {
    updateRate: readStoredInt('updateRate', 1),
    exportRange: {
      ...DEFAULT_EXPORT_RANGE,
      ...(readStoredJson('exportRange', {}) || {}),
    },
    exportCodec: localStorage.getItem('exportCodec') || 'prores_ks',
    globalDefaults,
    aspectRatio: localStorage.getItem('aspectRatio') || '16:9',
  }
}

export { DEFAULT_EXPORT_RANGE, DEFAULT_GLOBAL_DEFAULTS }
