import {
  DEFAULT_EXPORT_RANGE,
  DEFAULT_GLOBAL_DEFAULTS,
} from '../lib/template-snapshot'

let isUpdatingFromConfig = false
let isUpdatingFromTimeline = false

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

export function readStoredJson(key, fallback) {
  const value = localStorage.getItem(key)
  if (!value) return fallback

  try {
    return JSON.parse(value)
  } catch {
    return fallback
  }
}

export function readStoredInt(key, fallback) {
  const value = parseInt(localStorage.getItem(key) || `${fallback}`, 10)
  return Number.isFinite(value) ? value : fallback
}

export function persistSerializable(key, value) {
  if (value === null || value === undefined) {
    localStorage.removeItem(key)
    return
  }

  localStorage.setItem(key, JSON.stringify(value))
}

export function cloneSerializable(value) {
  return JSON.parse(JSON.stringify(value))
}

export function hasSerializableChanged(left, right) {
  return JSON.stringify(left) !== JSON.stringify(right)
}

export function updateConfigPersistence(state) {
  localStorage.setItem('editorConfig', JSON.stringify(state.config))

  if (state.lastRenderedConfig) {
    state.hasUnrenderedChanges = hasSerializableChanged(
      state.config,
      state.lastRenderedConfig,
    )
    return
  }

  state.hasUnrenderedChanges = true
}

export function readStoredConfig() {
  const savedConfig = localStorage.getItem('editorConfig')
  if (savedConfig) {
    try {
      const parsed = JSON.parse(savedConfig)
      if (parsed && parsed.scene) return parsed
    } catch {
      console.warn('Failed to parse saved config, using default')
    }
  }

  return DEFAULT_CONFIG
}

export function beginConfigUpdate() {
  const wasUpdating = isUpdatingFromConfig
  isUpdatingFromConfig = true
  return wasUpdating
}

export function endConfigUpdateSoon() {
  setTimeout(() => {
    isUpdatingFromConfig = false
  }, 100)
}

export function isConfigUpdateInProgress() {
  return isUpdatingFromConfig
}

export function isUpdatingFromTimelineFlag() {
  return isUpdatingFromTimeline
}

export function readStoredTemplateSettings() {
  return {
    updateRate: readStoredInt('updateRate', 1),
    exportRange: {
      ...DEFAULT_EXPORT_RANGE,
      ...(readStoredJson('exportRange', {}) || {}),
    },
    exportCodec: localStorage.getItem('exportCodec') || 'prores_ks',
    globalDefaults: {
      ...DEFAULT_GLOBAL_DEFAULTS,
      ...(readStoredJson('globalDefaults', {}) || {}),
    },
    aspectRatio: localStorage.getItem('aspectRatio') || '16:9',
  }
}

export { DEFAULT_EXPORT_RANGE, DEFAULT_GLOBAL_DEFAULTS }
