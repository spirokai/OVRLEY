import { dirname, isAbsolute } from '@tauri-apps/api/path'
import { getPreference, setPreference } from '@/lib/preferences-store'

export const LAST_RENDER_OUTPUT_DIR_KEY = 'last-render-output-dir'

/**
 * Returns the output extension owned by a render mode.
 * @param {'transparent'|'composite'} exportMode - Render mode.
 * @returns {string} Required extension without a leading dot.
 */
export function getRenderOutputExtension(exportMode) {
  return exportMode === 'composite' ? 'mp4' : 'mov'
}

/**
 * Normalizes only the final filename component of an absolute path.
 * @param {string} outputPath - User-supplied absolute path.
 * @param {'transparent'|'composite'} exportMode - Render mode.
 * @returns {string} Path with the mode-owned final extension.
 */
export function normalizeRenderOutputPath(outputPath, exportMode) {
  const extension = getRenderOutputExtension(exportMode)
  const separatorIndex = Math.max(outputPath.lastIndexOf('/'), outputPath.lastIndexOf('\\'))
  const directory = separatorIndex >= 0 ? outputPath.slice(0, separatorIndex + 1) : ''
  const filename = separatorIndex >= 0 ? outputPath.slice(separatorIndex + 1) : outputPath
  const stem = filename.replace(/\.[^.]*$/, '')
  return `${directory}${stem}.${extension}`
}

/**
 * Loads the optional remembered render directory.
 * @returns {Promise<string|undefined>} Valid absolute directory or absence.
 */
export async function loadRememberedRenderDirectory() {
  let value
  try {
    value = await getPreference(LAST_RENDER_OUTPUT_DIR_KEY)
  } catch (error) {
    console.warn('Could not read the remembered render output directory:', error)
    return undefined
  }

  if (value === undefined || value === null) {
    return undefined
  }
  if (typeof value !== 'string' || !value.trim() || !(await isAbsolute(value))) {
    throw new Error('The remembered render output directory is malformed.')
  }
  return value
}

/**
 * Persists the accepted output's parent directory without affecting render state.
 * @param {string} outputPath - Accepted absolute output path.
 * @returns {Promise<void>}
 */
export async function rememberAcceptedRenderOutput(outputPath) {
  try {
    await setPreference(LAST_RENDER_OUTPUT_DIR_KEY, await dirname(outputPath))
  } catch (error) {
    console.warn('Could not remember the render output directory:', error)
  }
}
