/**
 * Implements API helpers for backend.
 */

// Check if the Tauri IPC runtime is actually available.

/**
 * Checks whether is tauri.
 * @returns {boolean} Whether the condition is satisfied.
 */
const isTauri = () => typeof window !== 'undefined' && typeof window.__TAURI_INTERNALS__ !== 'undefined'

/**
 * Returns invoke.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function getInvoke() {
  if (!isTauri()) return null
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
  const invoke = await requireInvoke()

  // In Tauri, call Rust directly. No sidecar/socket probing remains.
  try {
    const result = await invoke(tauriCmd, tauriArgs)
    return JSON.parse(result)
  } catch (e) {
    console.error(`[Backend] Tauri bridge error for ${tauriCmd}:`, e)
    // Normalize errors to standard Error objects
    if (typeof e === 'string') {
      throw new Error(e)
    }
    if (e instanceof Error) {
      throw e
    }
    throw new Error(e?.message || e?.toString() || 'Unknown fetch error')
  }
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
 * Returns platform info.
 * @returns {Promise<object>} Promise resolving to the operation result.
 */
export async function getPlatformInfo() {
  const invoke = await requireInvoke()
  const payload = await invoke('backend_current_os')
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

/**
 * Lists available fonts.
 * @returns {Promise<Array<*>>} Promise resolving to the operation result.
 */
export async function listAvailableFonts() {
  const invoke = await getInvoke()
  if (invoke) {
    const payload = await invoke('backend_list_system_fonts')
    const fonts = typeof payload === 'string' ? JSON.parse(payload) : payload
    return Array.isArray(fonts) ? sortFontNames(fonts) : []
  }

  if (typeof window !== 'undefined' && typeof window.queryLocalFonts === 'function') {
    try {
      const fonts = await window.queryLocalFonts()
      return sortFontNames(fonts.map((font) => font.family || font.fullName || font.postscriptName || ''))
    } catch (error) {
      console.warn('Local font access unavailable in browser:', error)
    }
  }

  return []
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

/**
 * Opens downloads.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openDownloads() {
  return apiCall('backend_open_downloads', {})
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
  const invoke = await requireInvoke()
  return invoke('default_template_save_path', { filename })
}

/**
 * Writes template file.
 *
 * @param {*} path - Filesystem path for the target resource.
 * @param {*} contents - Serialized file contents to write.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function writeTemplateFile(path, contents) {
  const invoke = await requireInvoke()
  return invoke('write_template_file', { path, contents })
}

/**
 * Writes parse debug file.
 *
 * @param {*} filename - Target filename for the operation.
 * @param {*} contents - Serialized file contents to write.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function writeParseDebugFile(filename, contents) {
  const invoke = await requireInvoke()
  return invoke('write_parse_debug_file', { filename, contents })
}

/**
 * Probes video for metadata using ffprobe.
 *
 * @param {string} filePath - Path to the video file.
 * @returns {Promise<object>} Promise resolving to video metadata.
 */
export async function probeVideo(filePath) {
  const invoke = await requireInvoke()
  const result = await invoke('backend_probe_video', { filePath })
  return typeof result === 'string' ? JSON.parse(result) : result
}

/**
 * Detects available ffmpeg MP4 codecs and hardware acceleration methods.
 *
 * @returns {Promise<object>} Promise resolving to available codec flags.
 */
export async function detectCodecs() {
  const invoke = await requireInvoke()
  const result = await invoke('backend_detect_codecs', {})
  return typeof result === 'string' ? JSON.parse(result) : result
}
