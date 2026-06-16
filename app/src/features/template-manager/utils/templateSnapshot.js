/**
 * Template snapshot utilities for OVRLEY template files.
 *
 * Durable template normalization lives in the template-state seam. This module
 * focuses on file-oriented concerns: payload stamping, payload validation,
 * stringification, download, and structural state comparison.
 */

import { deepEqual } from '@/store/store-utils'
import { createDurableTemplateState } from '@/lib/template/template-state'
import { TEMPLATE_FILE_FORMAT, TEMPLATE_FILE_VERSION } from '@/lib/template/template-constants'

export { normalizeTemplateConfig } from '@/lib/template/template-normalization'
export { DEFAULT_GLOBAL_DEFAULTS } from '@/lib/template/template-constants'

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
 * Creates durable template state for save-status tracking and file output.
 *
 * @param {object} options - Structured options for the helper.
 * @param {*} options.config - Overlay template configuration data.
 * @param {*} options.globalDefaults - Value for global defaults.
 * @returns {object} Durable template state.
 */
export function createTemplateState({ config, globalDefaults }) {
  return createDurableTemplateState({ config, globalDefaults })
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
 * Normalizes template file payload to durable in-memory template state.
 *
 * @param {*} rawTemplate - Value for raw template.
 * @returns {object} Normalized durable template state plus optional name.
 */
export function normalizeTemplateFilePayload(rawTemplate) {
  if (!rawTemplate || typeof rawTemplate !== 'object') {
    throw new Error('Template file is empty or invalid.')
  }

  if (rawTemplate.format !== TEMPLATE_FILE_FORMAT || !rawTemplate.config || !rawTemplate.settings) {
    throw new Error('Unsupported template file format.')
  }

  if (rawTemplate.version !== TEMPLATE_FILE_VERSION) {
    throw new Error(`Unsupported template file version: ${rawTemplate.version}. Expected ${TEMPLATE_FILE_VERSION}.`)
  }

  const normalizedState = createDurableTemplateState({
    config: rawTemplate.config,
    globalDefaults: rawTemplate.settings.globalDefaults,
  })

  return {
    ...normalizedState,
    name: rawTemplate.name || null,
  }
}

/**
 * Compares template state objects for structural equality.
 *
 * @param {*} left - Left-hand template state.
 * @param {*} right - Right-hand template state.
 * @returns {boolean} Whether the two template states are structurally equivalent.
 */
export function templateStatesEqual(left, right) {
  return deepEqual(left, right)
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
