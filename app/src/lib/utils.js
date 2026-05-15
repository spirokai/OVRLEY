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
