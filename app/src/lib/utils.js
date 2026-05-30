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

/**
 * Checks whether a DOM target is inside an interactive element (input, textarea,
 * select, button, link, slider, or contenteditable). Useful for keyboard shortcut
 * guards that should be suppressed while the user is typing.
 *
 * @param {EventTarget} target - DOM event target to inspect.
 * @returns {boolean} True if target is inside an interactive element.
 */
export function isInteractiveElement(target) {
  if (!(target instanceof HTMLElement)) return false
  return Boolean(target.closest('input, textarea, select, button, a, [role="slider"], [contenteditable="true"]'))
}
