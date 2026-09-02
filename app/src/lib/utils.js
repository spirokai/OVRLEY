/**
 * Provides shared utils utilities for the app.
 */

import { clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

/**
 * Merges class name inputs into a single Tailwind-safe class string.
 *
 * @param {*} inputs - Value for inputs.
 * @returns {*} Result produced by the helper.
 */
export function cn(...inputs) {
  return twMerge(clsx(inputs))
}

export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/** @param {string} path Native path. @returns {string|null} Final path component. */
export function filenameFromSelectedPath(path) {
  if (!path) return null
  return String(path).split(/[\\/]/).filter(Boolean).at(-1) || null
}

/** @param {string} path Native file path. @returns {string} Parent directory. */
export function directoryFromSelectedPath(path) {
  const value = String(path)
  const separatorIndex = Math.max(value.lastIndexOf('/'), value.lastIndexOf('\\'))
  return separatorIndex < 0 ? '' : value.slice(0, separatorIndex)
}

/** @param {string} directory Native directory. @param {string} filename Filename only. @returns {string} Joined path. */
export function pathInDirectory(directory, filename) {
  if (!directory || !filename) throw new Error('Directory and filename are required')
  const separator = String(directory).includes('\\') ? '\\' : '/'
  return `${String(directory).replace(/[\\/]$/, '')}${separator}${filename}`
}

/**
 * Checks whether a DOM target is inside an interactive element (input, textarea,
 * select, button, link, ARIA listbox control, slider, or contenteditable). Useful for keyboard shortcut
 * guards that should be suppressed while the user is typing.
 *
 * @param {EventTarget} target - DOM event target to inspect.
 * @returns {boolean} True if target is inside an interactive element.
 */
export function isInteractiveElement(target) {
  if (!(target instanceof Element)) return false
  return Boolean(
    target.closest(
      'input, textarea, select, button, a, [role="combobox"], [role="listbox"], [role="option"], [role="slider"], [contenteditable="true"]',
    ),
  )
}
