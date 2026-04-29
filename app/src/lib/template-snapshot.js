/**
 * Provides shared template snapshot utilities for the app.
 */

import { normalizeColorFields } from './color-utils'

export const TEMPLATE_FILE_FORMAT = 'cyclemetry-template'
export const TEMPLATE_FILE_VERSION = 1

export const DEFAULT_EXPORT_RANGE = {
  type: 'all',
  from: 0,
  to: 0,
  fromTime: '00:00:00',
  toTime: '00:00:00',
}

export const DEFAULT_GLOBAL_DEFAULTS = {
  font_values: 'Arial.ttf',
  font_text: 'Arial.ttf',
  color_values: '#ffffff',
  color_text: '#ffffff',
  color_icons: '#ffffff',
  border_color: '#000000',
  border_thickness: 0,
  border_strength: 0,
  border_distance: 0,
  shadow_color: '#000000',
  shadow_strength: 0,
  shadow_distance: 0,
  opacity: 1,
  scale: 1,
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

/**
 * Handles sanitize template filename.
 *
 * @param {*} name - Value for name.
 * @returns {*} Result produced by the helper.
 */
export function sanitizeTemplateFilename(name) {
  const normalized = String(name || 'cyclemetry_template')
    .trim()
    .replace(/\.[^/.]+$/, '')
    .replace(/[^a-zA-Z0-9._-]+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '')

  return `${normalized || 'cyclemetry_template'}.json`
}

/**
 * Creates template state.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.config - Overlay template configuration data.
 * @param {*} options.globalDefaults - Value for global defaults.
 * @param {*} options.updateRate - Metric sampling rate used during export.
 * @param {*} options.exportRange - Requested export start and end bounds.
 * @param {*} options.exportCodec - Selected export codec identifier.
 * @param {*} options.aspectRatio - Value for aspect ratio.
 * @returns {object} Derived data structure for downstream use.
 */
export function createTemplateState({
  config,
  globalDefaults,
  updateRate,
  exportRange,
  exportCodec,
  aspectRatio,
}) {
  return {
    config: cloneSerializable(config),
    settings: {
      globalDefaults: {
        ...DEFAULT_GLOBAL_DEFAULTS,
        ...normalizeColorFields(cloneSerializable(globalDefaults) || {}),
      },
      updateRate: Number.isFinite(updateRate) ? updateRate : 1,
      exportRange: {
        ...DEFAULT_EXPORT_RANGE,
        ...(cloneSerializable(exportRange) || {}),
      },
      exportCodec: exportCodec || 'prores_ks',
      aspectRatio: aspectRatio || '16:9',
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
export function normalizeTemplateFilePayload(rawTemplate, fallbackState = {}) {
  if (!rawTemplate || typeof rawTemplate !== 'object') {
    throw new Error('Template file is empty or invalid.')
  }

  if (rawTemplate.config && rawTemplate.settings) {
    return {
      ...createTemplateState({
        config: rawTemplate.config,
        globalDefaults:
          rawTemplate.settings.globalDefaults || fallbackState.globalDefaults,
        updateRate: rawTemplate.settings.updateRate ?? fallbackState.updateRate,
        exportRange:
          rawTemplate.settings.exportRange || fallbackState.exportRange,
        exportCodec:
          rawTemplate.settings.exportCodec || fallbackState.exportCodec,
        aspectRatio:
          rawTemplate.settings.aspectRatio || fallbackState.aspectRatio,
      }),
      name: rawTemplate.name || null,
    }
  }

  if (rawTemplate.scene) {
    return {
      ...createTemplateState({
        config: rawTemplate,
        globalDefaults: fallbackState.globalDefaults,
        updateRate: fallbackState.updateRate,
        exportRange: fallbackState.exportRange,
        exportCodec: fallbackState.exportCodec,
        aspectRatio: fallbackState.aspectRatio,
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
