/**
 * Template snapshot utilities for OVRLEY template files.
 *
 * Durable template normalization lives in the template-state seam. This module
 * focuses on file-oriented concerns: payload stamping, payload validation,
 * stringification, download, and structural state comparison.
 */

import { createDurableTemplateState } from '@/lib/template/template-state'
import { RASTER_KEYS, TEMPLATE_FILE_FORMAT, TEMPLATE_FILE_VERSION } from '@/lib/template/template-constants'
import { loadSelectedRaster } from '@/api/backend'
import { attachRasterLoadResults } from '@/lib/widget/raster-resources'
import { prepareDocumentFonts } from '@/lib/font-resources'

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

function validateTemplateRasters(config) {
  if (!Array.isArray(config.rasters)) throw new Error('Raster collection must be an array')

  const rasterIds = new Set()
  for (const raster of config.rasters) {
    if (!raster || typeof raster !== 'object' || Array.isArray(raster)) throw new Error('Invalid raster config')
    if (Object.hasOwn(raster, 'resourceId') || Object.hasOwn(raster, 'resourceErrorCode')) {
      throw new Error('Template raster resources must not be persisted.')
    }
    const keys = Object.keys(raster)
    if (keys.length !== RASTER_KEYS.length) throw new Error('Invalid raster config')
    for (const key of keys) {
      if (!RASTER_KEYS.includes(key)) throw new Error('Invalid raster config')
    }
    if (typeof raster.id !== 'string' || !/^[A-Za-z0-9_-]+$/.test(raster.id)) throw new Error('Invalid raster id')
    for (const key of ['x', 'y', 'width', 'height', 'rotation', 'opacity']) {
      if (!Number.isFinite(raster[key])) throw new Error(`Invalid raster ${key}`)
    }
    if (raster.width <= 0 || raster.height <= 0 || raster.opacity < 0 || raster.opacity > 1) throw new Error('Invalid raster dimensions or opacity')
    if (raster.path !== null && (typeof raster.path !== 'string' || !/^(?:[a-zA-Z]:[\\/]|\\\\[^\\]+\\|\/)/.test(raster.path))) {
      throw new Error('Raster path must be absolute or null')
    }
    if (rasterIds.has(raster.id)) throw new Error('Raster widget IDs must be unique')
    rasterIds.add(raster.id)
  }
  for (const category of ['backdrops', 'labels', 'values', 'plots']) {
    if (config[category] === undefined) continue
    for (const widget of config[category]) {
      if (rasterIds.has(widget.id)) throw new Error('Raster widget IDs must be unique')
    }
  }
}

/** @param {object} value Cloned file input. @returns {void} Migrates legacy bundled font identities at ingress. */
function migrateFontIdentities(value) {
  for (const [key, item] of Object.entries(value)) {
    if (
      ['font', 'label_font', 'min_max_label_font', 'font_text', 'font_values'].includes(key) &&
      ['Inter ExtraBold.ttf', 'Inter ExtraBold'].includes(item)
    ) {
      value[key] = 'Inter.ttf'
    } else if (key === 'font_family' && item === 'Inter ExtraBold') {
      value[key] = 'Inter'
    } else if (item !== null && typeof item === 'object') {
      migrateFontIdentities(item)
    }
  }
}

/** @param {object} payload Cloned template file input. @returns {void} Migrates documented legacy absence, then validates label typography once. */
function normalizeTemplateFontInput(payload) {
  migrateFontIdentities(payload)
  for (const label of payload.config.labels ?? []) {
    if (!Object.hasOwn(label, 'font_weight')) label.font_weight = 400
    if (!Object.hasOwn(label, 'italic')) label.italic = false
    if (!Object.hasOwn(label, 'letter_spacing')) label.letter_spacing = 0
    if (!Number.isFinite(label.font_weight) || label.font_weight < 1 || label.font_weight > 1000) {
      throw new Error('Label font_weight must be a finite number from 1 to 1000')
    }
    if (typeof label.italic !== 'boolean') throw new Error('Label italic must be a boolean')
    if (!Number.isFinite(label.letter_spacing) || !Number.isFinite(Math.fround(label.letter_spacing))) {
      throw new Error('Label letter_spacing must be a finite 32-bit number')
    }
  }
}

function migrateTemplatePayload(rawTemplate) {
  switch (rawTemplate.version) {
    case 2:
      if (Object.hasOwn(rawTemplate.config, 'rasters')) throw new Error('Version 2 template cannot contain rasters.')
      return {
        ...rawTemplate,
        version: TEMPLATE_FILE_VERSION,
        config: {
          ...rawTemplate.config,
          rasters: [],
        },
      }
    case TEMPLATE_FILE_VERSION:
      return rawTemplate
    default:
      throw new Error(`Unsupported template file version: ${rawTemplate.version}. Expected ${TEMPLATE_FILE_VERSION}.`)
  }
}

/**
 * Normalizes template file payload to durable in-memory template state.
 *
 * @param {*} rawTemplate - Value for raw template.
 * @returns {object} Normalized durable template state plus optional name.
 */
export function normalizeTemplateFilePayload(rawTemplate) {
  if (!rawTemplate || typeof rawTemplate !== 'object' || Array.isArray(rawTemplate)) {
    throw new Error('Template file is empty or invalid.')
  }

  if (rawTemplate.format !== TEMPLATE_FILE_FORMAT) {
    throw new Error('Unsupported template file format.')
  }

  if (!rawTemplate.config || typeof rawTemplate.config !== 'object' || Array.isArray(rawTemplate.config)) {
    throw new Error('Template config must be an object.')
  }

  if (!rawTemplate.settings || typeof rawTemplate.settings !== 'object' || Array.isArray(rawTemplate.settings)) {
    throw new Error('Template settings must be an object.')
  }

  if (
    !rawTemplate.settings.globalDefaults ||
    typeof rawTemplate.settings.globalDefaults !== 'object' ||
    Array.isArray(rawTemplate.settings.globalDefaults)
  ) {
    throw new Error('Template settings.globalDefaults must be an object.')
  }

  const migratedTemplate = migrateTemplatePayload(structuredClone(rawTemplate))
  normalizeTemplateFontInput(migratedTemplate)
  validateTemplateRasters(migratedTemplate.config)
  const normalizedState = createDurableTemplateState({
    config: migratedTemplate.config,
    globalDefaults: migratedTemplate.settings.globalDefaults,
  })

  return {
    ...normalizedState,
    name: migratedTemplate.name || null,
  }
}

async function loadTemplateRaster(raster) {
  let result
  try {
    const resource = await loadSelectedRaster(raster.path)
    result = { status: 'ready', resourceId: resource.resourceId }
  } catch (error) {
    if (!error.code) throw error
    result = { status: 'error', errorCode: error.code }
  }
  return [raster.id, result]
}

/**
 * Normalizes one standalone template and resolves each populated raster path
 * into immutable session resource state.
 *
 * Resource failures are retained per widget so unrelated template content can
 * enter the editor and the raster can be replaced.
 *
 * @param {object} rawTemplate - Parsed standalone template payload.
 * @returns {Promise<{templateState: object}>} Prepared editor document state.
 */
export async function prepareTemplateFilePayload(rawTemplate) {
  const templateState = normalizeTemplateFilePayload(rawTemplate)
  const pendingRasters = []
  for (const raster of templateState.config.rasters) {
    if (raster.path === null) continue
    const pendingRaster = loadTemplateRaster(raster)
    pendingRasters.push(pendingRaster)
  }
  const [entries] = await Promise.all([Promise.all(pendingRasters), prepareDocumentFonts(templateState)])
  const rasterLoadResults = Object.fromEntries(entries)
  const config = attachRasterLoadResults(templateState.config, rasterLoadResults)

  return {
    templateState: {
      ...templateState,
      config,
    },
  }
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
