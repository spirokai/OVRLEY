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

function cloneSerializable(value) {
  if (value === undefined) return undefined
  return JSON.parse(JSON.stringify(value))
}

export function sanitizeTemplateFilename(name) {
  const normalized = String(name || 'cyclemetry_template')
    .trim()
    .replace(/\.[^/.]+$/, '')
    .replace(/[^a-zA-Z0-9._-]+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '')

  return `${normalized || 'cyclemetry_template'}.json`
}

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

export function createTemplateFilePayload(state, meta = {}) {
  return {
    format: TEMPLATE_FILE_FORMAT,
    version: TEMPLATE_FILE_VERSION,
    name: meta.name || null,
    savedAt: new Date().toISOString(),
    ...createTemplateState(state),
  }
}

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

export function templateStatesEqual(left, right) {
  return JSON.stringify(left) === JSON.stringify(right)
}

export function stringifyTemplateFile(payload) {
  return JSON.stringify(payload, null, 2)
}

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
