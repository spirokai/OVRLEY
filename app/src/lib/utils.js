/**
 * Small, domain-independent helpers shared throughout the app.
 */

import { clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

/**
 * Merges class name inputs into a single Tailwind-safe class string.
 *
 * @param {...import('clsx').ClassValue} inputs Class names and conditional class inputs.
 * @returns {string} Merged class names.
 */
export function cn(...inputs) {
  return twMerge(clsx(inputs))
}

/** @param {number} value Value to constrain. @param {number} min Lower bound. @param {number} max Upper bound. @returns {number} Clamped value. */
export function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

/** @param {object|null|undefined} source Optional source record. @param {string[]} keys Fields to copy. @returns {object} Present fields, excluding undefined. */
export function pickDefined(source, keys) {
  const result = {}
  for (const key of keys) {
    if (source?.[key] !== undefined) result[key] = source[key]
  }
  return result
}
