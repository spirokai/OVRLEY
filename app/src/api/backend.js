/**
 * Implements API helpers for backend.
 */

// Check if the Tauri IPC runtime is actually available.

/**
 * Checks whether is tauri.
 * @returns {boolean} Whether the condition is satisfied.
 */
const isTauri = () =>
  typeof window !== 'undefined' &&
  typeof window.__TAURI_INTERNALS__ !== 'undefined'

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
 * @param {*} method - HTTP method used for the backend request.
 * @param {*} tauriCmd - Tauri command name to invoke.
 * @param {*} tauriArgs - Argument payload passed to the Tauri command.
 * @param {*} fetchPath - HTTP endpoint used in browser mode.
 * @param {*} fetchOptions - Fetch options.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
async function apiCall(
  method,
  tauriCmd,
  tauriArgs,
  fetchPath,
  fetchOptions = {},
) {
  const invoke = await getInvoke()

  // In Tauri, call Rust directly. No sidecar/socket probing remains.
  if (invoke) {
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
      throw new Error(e?.message || e?.toString() || 'Unknown Tauri error')
    }
  }

  // Fallback to fetch (Only for web development mode / non-Tauri)
  const baseUrl = 'http://localhost:31337'
  const url = fetchPath.startsWith('http')
    ? fetchPath
    : `${baseUrl}${fetchPath}`

  console.log(`[Backend] Web Fetch: ${method} ${url}`)
  try {
    const response = await fetch(url, {
      method,
      headers:
        fetchOptions.headers ||
        (method === 'POST' ? { 'Content-Type': 'application/json' } : {}),
      body: fetchOptions.body,
      signal: fetchOptions.signal,
    })

    if (!response.ok) {
      const errorText = await response.text().catch(() => '')
      throw new Error(errorText || `HTTP error! status: ${response.status}`)
    }

    return response.json()
  } catch (e) {
    console.error(`[Backend] Fetch to ${url} failed:`, e)
    // Normalize and add specific message about connection failure
    if (
      e.name === 'TypeError' &&
      (e.message === 'Load failed' || e.message === 'Failed to fetch')
    ) {
      throw new Error(
        'Could not connect to backend. Please ensure the server is running.',
      )
    }
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
  return apiCall('GET', 'backend_health', {}, '/api/health')
}

/**
 * Handles socket ready.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function socketReady() {
  const invoke = await getInvoke()
  if (invoke) {
    return true
  }
  return false
}

/**
 * Handles generate demo.
 *
 * @param {*} config - Overlay template configuration data.
 * @param {*} parsedActivity - Normalized activity payload used by the app.
 * @param {*} second - Preview or export time in seconds.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function generateDemo(config, parsedActivity, second) {
  const safeConfig = safeJsonStringify(config)
  const safeParsedActivity = safeJsonStringify(parsedActivity)
  return apiCall(
    'POST',
    'backend_demo',
    {
      configJson: safeConfig,
      parsedActivityJson: safeParsedActivity,
      second,
    },
    '/api/demo',
    {
      body: safeJsonStringify({ config, parsedActivity, second }),
    },
  )
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
  return apiCall(
    'POST',
    'backend_render',
    {
      configJson: safeConfig,
      parsedActivityJson: safeParsedActivity,
    },
    '/api/render-video',
    {
      body: safeJsonStringify({ config, parsedActivity }),
    },
  )
}

/**
 * Handles detect browser platform os.
 * @returns {*} Result produced by the helper.
 */
function detectBrowserPlatformOs() {
  const platform =
    navigator.userAgentData?.platform ||
    navigator.platform ||
    navigator.userAgent

  if (/mac/i.test(platform)) {
    return 'macos'
  }
  if (/win/i.test(platform)) {
    return 'windows'
  }
  if (/linux/i.test(platform)) {
    return 'linux'
  }

  return 'unknown'
}

/**
 * Returns platform info.
 * @returns {Promise<object>} Promise resolving to the operation result.
 */
export async function getPlatformInfo() {
  const invoke = await getInvoke()
  if (invoke) {
    const payload = await invoke('backend_current_os')
    return typeof payload === 'string' ? JSON.parse(payload) : payload
  }

  return { os: detectBrowserPlatformOs() }
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
    .sort((left, right) =>
      left.localeCompare(right, undefined, { sensitivity: 'base' }),
    )
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

  if (
    typeof window !== 'undefined' &&
    typeof window.queryLocalFonts === 'function'
  ) {
    try {
      const fonts = await window.queryLocalFonts()
      return sortFontNames(
        fonts.map(
          (font) => font.family || font.fullName || font.postscriptName || '',
        ),
      )
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
  return apiCall('GET', 'backend_progress', {}, '/api/render-progress')
}

/**
 * Checks whether cancel render.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function cancelRender() {
  return apiCall('POST', 'backend_cancel', {}, '/api/cancel-render')
}

/**
 * Opens downloads.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openDownloads() {
  return apiCall('POST', 'backend_open_downloads', {}, '/api/open-downloads')
}

/**
 * Opens video.
 *
 * @param {*} filename - Target filename for the operation.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openVideo(filename) {
  return apiCall(
    'POST',
    'backend_open_video',
    { filename },
    '/api/open-video',
    {
      body: safeJsonStringify({ filename }),
    },
  )
}

/**
 * Returns image url.
 *
 * @param {*} filename - Target filename for the operation.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function getImageUrl(filename) {
  const invoke = await getInvoke()
  if (invoke) {
    try {
      return await invoke('backend_image_data', { filename })
    } catch (e) {
      console.warn(
        'Failed to fetch image data via Tauri, falling back to fetch',
        e,
      )
    }
  }

  // Fallback to fetch (TCP mode)
  return `http://localhost:31337/images/${filename}`
}

/**
 * Lists templates.
 * @returns {Promise<Array<*>>} Promise resolving to the operation result.
 */
export async function listTemplates() {
  return apiCall('GET', 'backend_list_templates', {}, '/api/templates')
}

/**
 * Returns template.
 *
 * @param {*} filename - Target filename for the operation.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function getTemplate(filename) {
  return apiCall(
    'GET',
    'backend_get_template',
    { filename },
    `/templates/${filename}`,
  )
}

/**
 * Handles save template.
 *
 * @param {*} filename - Target filename for the operation.
 * @param {*} config - Overlay template configuration data.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function saveTemplate(filename, config) {
  const safeConfig = safeJsonStringify(config)
  return apiCall(
    'POST',
    'backend_save_template',
    {
      filename,
      config: safeConfig,
    },
    '/api/save-template',
    {
      body: safeJsonStringify({ filename, config }),
    },
  )
}

/**
 * Opens templates folder.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function openTemplatesFolder() {
  return apiCall('POST', 'backend_open_templates', {}, '/api/open-templates')
}

/**
 * Returns default template save path.
 *
 * @param {*} filename - Target filename for the operation.
 * @returns {Promise<*>} Promise resolving to the operation result.
 */
export async function getDefaultTemplateSavePath(filename) {
  const invoke = await getInvoke()
  if (!invoke) {
    throw new Error(
      'Native template save path is only available in desktop app',
    )
  }

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
  const invoke = await getInvoke()
  if (!invoke) {
    throw new Error('Native template save is only available in desktop app')
  }

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
  const invoke = await getInvoke()
  if (invoke) {
    return invoke('write_parse_debug_file', { filename, contents })
  }

  const response = await fetch('/api/parse-debug', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ filename, contents }),
  })

  if (!response.ok) {
    const errorPayload = await response.json().catch(() => null)
    throw new Error(
      errorPayload?.error ||
        `Failed to write parse debug file: ${response.status}`,
    )
  }

  const data = await response.json()
  return data.path
}
