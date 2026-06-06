/**
 * File dialog helpers for template import/export operations.
 * Pure functions - no React dependencies.
 */

import { selectBrowserFile } from '@/lib/file-dialog'

export const selectBrowserTemplateFile = () => selectBrowserFile('.json,application/json')

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
