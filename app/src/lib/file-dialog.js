import { open, save } from '@tauri-apps/plugin-dialog'
import { readSelectedFileBytes } from '@/api/backend'
import { getPreference, setPreference } from '@/lib/preferences-store'

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

function directoryFromSelectedPath(path) {
  return String(path).replace(/[\\/][^\\/]*$/, '')
}

function filenameFromSelectedPath(path) {
  return String(path).split(/[\\/]/).at(-1)
}

function pathInDirectory(directory, filename) {
  const separator = String(directory).includes('\\') ? '\\' : '/'
  return `${String(directory).replace(/[\\/]$/, '')}${separator}${filename}`
}

export async function openSinglePath(filters, options = {}) {
  const { defaultPath: initialDefaultPath, lastDirectoryKey } = options
  let defaultPath = initialDefaultPath

  if (lastDirectoryKey) {
    try {
      const saved = await getPreference(lastDirectoryKey)
      if (saved) defaultPath = saved
    } catch {
      // store unavailable — proceed without default path
    }
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
    try {
      const savedDirectory = await getPreference(lastDirectoryKey)
      const filename = filenameFromSelectedPath(defaultPath)
      if (savedDirectory && filename) resolvedDefaultPath = pathInDirectory(savedDirectory, filename)
    } catch {
      // store unavailable — retain the supplied default path
    }
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
