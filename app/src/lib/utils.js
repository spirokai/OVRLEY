/**
 * Provides shared utils utilities for the app.
 */

import { clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'
import i18n from '@/i18n'

/**
 * Formats tagged backend errors for display while leaving untagged messages unchanged.
 * @param {string} message - Backend error or progress message.
 * @returns {string} User-facing message.
 */
export function translateBackendError(message) {
  const projectCode = message.match(/\[project_error:([a-z_]+)\]/)?.[1]
  const projectKeys = {
    archive_size: 'archiveTooLarge',
    invalid_archive: 'invalidArchive',
    raster_unavailable: 'rasterUnavailable',
    save_failed: 'saveFailed',
  }
  if (projectCode && projectKeys[projectCode]) return i18n.t(`projects.${projectKeys[projectCode]}`)
  const match = message.match(/\[raster_error:([a-z_]+):([^\]]+)\]/)
  if (!match) return message
  const keys = {
    invalid_config: 'invalidConfig',
    no_image: 'noImage',
    missing_resource: 'renderUnavailable',
    relative_path: 'relativePath',
    missing: 'missing',
    unreadable: 'unreadable',
    unsupported_type: 'unsupportedType',
    encoded_size: 'encodedSize',
    resolution: 'resolution',
    decode: 'decode',
    missing_asset: 'missingAsset',
    corrupt_asset: 'corruptAsset',
  }
  const key = keys[match[1]]
  if (!key) return message
  const translated = i18n.t(`raster.errors.${key}`, { id: match[2] })
  return key === 'noImage' || key === 'renderUnavailable' || match[2] === 'configuration' ? translated : `${match[2]}: ${translated}`
}

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
