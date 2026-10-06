import { open, save } from '@tauri-apps/plugin-dialog'
import { dirname, isAbsolute } from '@tauri-apps/api/path'
import { readSelectedFileBytes } from '@/api/backend'
import { getOptionalPathPreference, getPreference, setPreference } from '@/lib/preferences-store'
import { directoryFromSelectedPath, filenameFromSelectedPath, pathInDirectory } from '@/lib/utils'

const LAST_RENDER_OUTPUT_DIR_KEY = 'last-render-output-dir'

/** @returns {Promise<string|undefined>} Optional remembered absolute render directory. */
export async function loadRememberedRenderDirectory() {
  let value
  try {
    value = await getPreference(LAST_RENDER_OUTPUT_DIR_KEY)
  } catch (error) {
    console.warn('Could not read the remembered render output directory:', error)
    return undefined
  }
  if (value === undefined || value === null) return undefined
  if (typeof value !== 'string' || !value.trim() || !(await isAbsolute(value))) {
    throw new Error('The remembered render output directory is malformed.')
  }
  return value
}

/** @param {string} outputPath Accepted absolute output. @returns {Promise<void>} */
export async function rememberAcceptedRenderOutput(outputPath) {
  try {
    await setPreference(LAST_RENDER_OUTPUT_DIR_KEY, await dirname(outputPath))
  } catch (error) {
    console.warn('Could not remember the render output directory:', error)
  }
}

export const selectBrowserFile = (accept) =>
  new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = accept
    input.onchange = () => resolve(input.files?.[0] ?? null)
    input.oncancel = () => resolve(null)
    input.click()
  })

export async function fileFromSelectedPath(selectedPath, fallbackName = 'file') {
  const bytes = await readSelectedFileBytes(selectedPath)
  const filename = String(selectedPath).split(/[/\\]/).pop() || fallbackName
  return new File([bytes], filename, { type: 'application/octet-stream' })
}

export async function openSinglePath(filters, options = {}) {
  const { defaultPath: initialDefaultPath, lastDirectoryKey } = options
  let defaultPath = initialDefaultPath

  if (lastDirectoryKey) {
    const savedDirectory = await getOptionalPathPreference(lastDirectoryKey)
    if (savedDirectory) defaultPath = savedDirectory
  }

  const selected = await open({
    multiple: false,
    filters,
    ...(defaultPath ? { defaultPath } : {}),
  })

  if (selected && lastDirectoryKey) {
    try {
      await setPreference(lastDirectoryKey, directoryFromSelectedPath(selected))
    } catch {
      // store may be unavailable
    }
  }

  return selected
}

/**
 * Opens the native directory picker.
 *
 * @param {{defaultPath?: string, lastDirectoryKey?: string}} [options] - Directory-picker options.
 * @returns {Promise<string|null>} Selected directory path or null when cancelled.
 */
export async function openDirectoryPath(options = {}) {
  const { defaultPath: initialDefaultPath, lastDirectoryKey } = options
  let defaultPath = initialDefaultPath

  if (lastDirectoryKey) {
    const savedDirectory = await getOptionalPathPreference(lastDirectoryKey)
    if (savedDirectory) defaultPath = savedDirectory
  }

  const selected = await open({
    directory: true,
    multiple: false,
    ...(defaultPath ? { defaultPath } : {}),
  })

  if (selected && lastDirectoryKey) {
    try {
      await setPreference(lastDirectoryKey, selected)
    } catch {
      // store may be unavailable
    }
  }

  return selected
}

/**
 * Opens the native save picker for a complete render target.
 *
 * @param {string} defaultPath - Current absolute output path.
 * @param {string} extension - The single allowed output extension.
 * @param {string} [filterName] User-facing file type name.
 * @param {{lastDirectoryKey?: string}} [options] Persistent directory preference.
 * @returns {Promise<string|null>} Selected path or null when cancelled.
 */
export async function saveSinglePath(defaultPath, extension, filterName = extension.toUpperCase(), options = {}) {
  const { lastDirectoryKey } = options
  let resolvedDefaultPath = defaultPath
  if (lastDirectoryKey) {
    const savedDirectory = await getOptionalPathPreference(lastDirectoryKey)
    const filename = filenameFromSelectedPath(defaultPath)
    if (savedDirectory && filename) resolvedDefaultPath = pathInDirectory(savedDirectory, filename)
  }
  const selected = await save({
    defaultPath: resolvedDefaultPath,
    filters: [{ name: filterName, extensions: [extension] }],
  })
  if (selected && lastDirectoryKey) {
    try {
      await setPreference(lastDirectoryKey, directoryFromSelectedPath(selected))
    } catch {
      // store may be unavailable
    }
  }
  return selected ?? null
}
