/**
 * File dialog helpers for template import/export operations.
 * Pure functions — no React dependencies.
 */

/**
 * Creates a hidden file input element and resolves with the selected file.
 * Falls back to browser file picker when Tauri dialog is unavailable.
 *
 * @returns {Promise<File|null>} The selected file, or null if cancelled.
 */
export const selectBrowserTemplateFile = () =>
  new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.json,application/json'
    input.onchange = () => resolve(input.files?.[0] ?? null)
    input.oncancel = () => resolve(null)
    input.click()
  })

/**
 * Extracts the filename from a full filesystem path.
 *
 * @param {string} path - Full filesystem path.
 * @returns {string} Extracted filename, or fallback if path is empty.
 */
export const getFilenameFromPath = (path) => {
  const segments = String(path || '').split(/[/\\]/)
  return segments[segments.length - 1] || 'ovrley_template.json'
}

/**
 * Strips the resource prefix from a template identifier.
 *
 * @param {string} templateId - Template identifier (e.g. "user:my_template").
 * @returns {string} Clean filename without prefix.
 */
export const getFilenameFromTemplateId = (templateId) => String(templateId || '').replace(/^(user:|built-in:)/, '')
