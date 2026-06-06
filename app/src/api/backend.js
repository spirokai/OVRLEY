/**
 * Implements API helpers for backend.
 */

import { formatFontLabel, setBundledRecommendedFonts } from '@/lib/fonts'

/**
 * Shared Tauri runtime detection.
 * Returns true when running inside a Tauri desktop shell (IPC available).
 * @returns {boolean}
 */
export function hasTauriRuntime() {
  return typeof window !== 'undefined' && typeof window.__TAURI_INTERNALS__ !== 'undefined'
}

/**
 * Returns invoke.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function getInvoke() {
  if (!hasTauriRuntime()) return null
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke
}

/**
 * Returns invoke or throws when desktop runtime is unavailable.
 * @returns {Promise<*>} Promise resolving to the Tauri invoke helper.
 */
async function requireInvoke() {
  const invoke = await getInvoke()
  if (!invoke) {
    throw new Error('OVRLEY desktop runtime is required.')
  }
  return invoke
}

/**
 * Normalizes values thrown by the Tauri bridge to standard Error instances.
 *
 * @param {*} error - Rejection value from the invoke bridge.
 * @param {string} fallbackMessage - Message used when no better detail exists.
 * @returns {Error} Normalized Error instance.
 */
function normalizeBackendError(error, fallbackMessage = 'Unknown backend error') {
  if (error instanceof Error) {
    return error
  }

  if (typeof error === 'string' && error.trim()) {
    return new Error(error)
  }

  if (error && typeof error === 'object' && typeof error.message === 'string' && error.message.trim()) {
    return new Error(error.message)
  }

  return new Error(fallbackMessage)
}

/**
 * Invokes a Tauri command and normalizes bridge errors.
 *
 * @param {string} tauriCmd - Tauri command name to invoke.
 * @param {object} tauriArgs - Argument payload passed to the Tauri command.
 * @returns {Promise<*>} Promise resolving to the raw command result.
 */
async function invokeCommand(tauriCmd, tauriArgs = {}) {
  const invoke = await requireInvoke()

  try {
    return await invoke(tauriCmd, tauriArgs)
  } catch (error) {
    console.error(`[Backend] Tauri bridge error for ${tauriCmd}:`, error)
    throw normalizeBackendError(error)
  }
}

/**
 * Serializes data to JSON while safely handling cyclic references.
 *
 * @param {*} obj - Value for obj.
 * @returns {*} Result produced by the helper.
 */
function safeJsonStringify(obj) {
  try {
    return JSON.stringify(obj)
  } catch {
    // Only use cycle breaking if normal stringify fails
    const seen = new WeakSet()
    return JSON.stringify(obj, (key, value) => {
      if (typeof value === 'object' && value !== null) {
        if (seen.has(value)) {
          return // Circular reference found
        }
        seen.add(value)
      }
      return value
    })
  }
}

/**
 * Handles api call.
 *
 * @param {*} tauriCmd - Tauri command name to invoke.
 * @param {*} tauriArgs - Argument payload passed to the Tauri command.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function apiCall(tauriCmd, tauriArgs) {
  // In Tauri, call Rust directly. No sidecar/socket probing remains.
  const result = await invokeCommand(tauriCmd, tauriArgs)
  return JSON.parse(result)
}

/**
 * Handles health check.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function healthCheck() {
  return apiCall('backend_health', {})
}

/**
 * Handles socket ready.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function socketReady() {
  return Boolean(await getInvoke())
}

/**
 * Renders video.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} parsedActivity - Normalized activity payload used by the app.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function renderVideo(config, parsedActivity) {
  const safeConfig = safeJsonStringify(config)
  const safeParsedActivity = safeJsonStringify(parsedActivity)
  return apiCall('backend_render', {
    configJson: safeConfig,
    parsedActivityJson: safeParsedActivity,
  })
}

/**
 * Renders a transparent PNG for a single preview second.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} parsedActivity - Normalized activity payload used by the app.
 * @param {number} second - Timeline second to render.
 * @returns {Promise<object>} Promise resolving to the generated preview metadata.
 */
export async function renderPreviewFrame(config, parsedActivity, second) {
  const safeConfig = safeJsonStringify(config)
  const safeParsedActivity = safeJsonStringify(parsedActivity)
  return apiCall('backend_render_preview_frame', {
    configJson: safeConfig,
    parsedActivityJson: safeParsedActivity,
    second,
  })
}

/**
 * Returns platform info.
 * @returns {Promise<object>} Promise resolving to the operation result.
 */
export async function getPlatformInfo() {
  const payload = await invokeCommand('backend_current_os')
  return typeof payload === 'string' ? JSON.parse(payload) : payload
}

/**
 * Sorts font names.
 *
 * @param {*} fonts - Value for fonts.
 * @returns {*} Result produced by the helper.
 */
function sortFontNames(fonts) {
  return [...new Set(fonts.filter(Boolean))]
    .map((font) => font.trim())
    .filter(Boolean)
    .sort((left, right) => left.localeCompare(right, undefined, { sensitivity: 'base' }))
}

function sortFontOptions(fonts) {
  const byId = new Map()

  fonts.forEach((font) => {
    const id = typeof font === 'string' ? font.trim() : String(font?.id || font?.name || '').trim()
    if (!id) {
      return
    }

    const option = {
      id,
      name: typeof font === 'object' && typeof font?.name === 'string' && font.name.trim() ? font.name.trim() : formatFontLabel(id),
    }

    const key = option.id.toLowerCase()
    if (!byId.has(key)) {
      byId.set(key, option)
    }
  })

  return [...byId.values()].sort((left, right) => left.name.localeCompare(right.name, undefined, { sensitivity: 'base' }))
}

/**
 * Lists available fonts.
 * @returns {Promise<Array<*>>} Promise resolving to the operation result.
 */
export async function listAvailableFonts() {
  const invoke = await getInvoke()
  if (invoke) {
    const payload = await invoke('backend_list_system_fonts')
    const fonts = typeof payload === 'string' ? JSON.parse(payload) : payload
    if (Array.isArray(fonts)) {
      setBundledRecommendedFonts([])
      return {
        recommendedFonts: [],
        systemFonts: sortFontNames(fonts),
      }
    }

    const recommendedFonts = sortFontOptions(fonts?.recommendedFonts || fonts?.bundledFonts || [])
    setBundledRecommendedFonts(recommendedFonts)
    return {
      recommendedFonts,
      systemFonts: sortFontNames(fonts?.systemFonts || []),
    }
  }

  if (typeof window !== 'undefined' && typeof window.queryLocalFonts === 'function') {
    try {
      const fonts = await window.queryLocalFonts()
      setBundledRecommendedFonts([])
      return {
        recommendedFonts: [],
        systemFonts: sortFontNames(fonts.map((font) => font.family || font.fullName || font.postscriptName || '')),
      }
    } catch (error) {
      console.warn('Local font access unavailable in browser:', error)
    }
  }

  setBundledRecommendedFonts([])
  return {
    recommendedFonts: [],
    systemFonts: [],
  }
}

/**
 * Returns render progress.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function getRenderProgress() {
  return apiCall('backend_progress', {})
}

/**
 * Checks whether cancel render.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function cancelRender() {
  return apiCall('backend_cancel', {})
}

async function openFolder(command) {
  return apiCall(command, {})
}

/**
 * Opens downloads.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openDownloads() {
  return openFolder('backend_open_downloads')
}

/**
 * Opens the user templates directory.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openTemplates() {
  return openFolder('backend_open_templates')
}

/**
 * Opens video.
 *
 * @param {*} filename - Target filename for the operation.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openVideo(filename) {
  return apiCall('backend_open_video', { filename })
}

/**
 * Lists templates.
 * @returns {Promise<Array<*>>} Promise resolving to the operation result.
 */
export async function listTemplates() {
  return apiCall('backend_list_templates', {})
}

/**
 * Returns template.
 *
 * @param {*} filename - Target filename for the operation.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function getTemplate(filename) {
  return apiCall('backend_get_template', { filename })
}

/**
 * @param {*} filename - Target filename for the operation.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function getDefaultTemplateSavePath(filename) {
  return invokeCommand('default_template_save_path', { filename })
}

/**
 * Writes template file.
 *
 * @param {*} path - Filesystem path for the target resource.
 * @param {*} contents - Serialized file contents to write.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function writeTemplateFile(path, contents) {
  return invokeCommand('write_template_file', { path, contents })
}

/**
 * Writes parse debug file.
 *
 * @param {*} filename - Target filename for the operation.
 * @param {*} contents - Serialized file contents to write.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function writeParseDebugFile(filename, contents) {
  return invokeCommand('write_parse_debug_file', { filename, contents })
}

/**
 * Imports a video into the local HTTP preview server.
 *
 * @param {string} path - Absolute path to the source video file.
 * @returns {Promise<object>} Promise resolving to preview URL, import ID, metadata, and warnings.
 */
export async function importPreviewVideo(path) {
  return apiCall('backend_import_preview_video', { path })
}

/**
 * Clears the currently registered local HTTP preview video.
 *
 * @returns {Promise<*>} Promise resolving when the preview has been cleared.
 */
export async function clearPreviewVideo() {
  return apiCall('backend_clear_preview_video', {})
}

/**
 * Returns current local HTTP preview server state.
 *
 * @returns {Promise<object|null>} Promise resolving to current preview server state or null.
 */
export async function getVideoState() {
  return apiCall('backend_get_video_state', {})
}

/**
 * Detects available ffmpeg MP4 codecs and hardware acceleration methods.
 *
 * @returns {Promise<object>} Promise resolving to available codec flags.
 */
export async function detectCodecs() {
  const result = await invokeCommand('backend_detect_codecs', {})
  return typeof result === 'string' ? JSON.parse(result) : result
}
