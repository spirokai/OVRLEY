/**
 * Color normalization and CSS theme color lookup.
 */

const THEME_COLOR_VARS = {
  background: '--theme-color-background',
  accent: '--theme-color-accent',
  aqua: '--theme-color-aqua',
  ice: '--theme-color-ice',
  success: '--theme-color-success',
  teal: '--theme-color-teal',
}

/**
 * Reads a named theme color or CSS custom property.
 *
 * @param {string} name Theme color name or CSS custom property.
 * @returns {string} Resolved color, or empty text without a browser document.
 */
export function getThemeColor(name) {
  const variableName = THEME_COLOR_VARS[name] || name

  if (typeof window === 'undefined' || typeof document === 'undefined') {
    return ''
  }

  return getComputedStyle(document.documentElement).getPropertyValue(variableName).trim()
}

/**
 * Normalizes hex color.
 *
 * @param {*} value - Input value processed by the helper.
 * @param {*} fallback - Fallback value returned when input is invalid.
 * @returns {*} Derived data structure for downstream use.
 */
export function normalizeHexColor(value, fallback = '#000000') {
  if (typeof value !== 'string') {
    return fallback
  }

  const trimmed = value.trim().replace(/^#/, '')

  if (/^[0-9a-fA-F]{3}$/.test(trimmed)) {
    return `#${trimmed
      .split('')
      .map((part) => part.repeat(2))
      .join('')
      .toLowerCase()}`
  }

  if (/^[0-9a-fA-F]{4}$/.test(trimmed)) {
    return `#${trimmed
      .slice(0, 3)
      .split('')
      .map((part) => part.repeat(2))
      .join('')
      .toLowerCase()}`
  }

  if (/^[0-9a-fA-F]{6}$/.test(trimmed)) {
    return `#${trimmed.toLowerCase()}`
  }

  if (/^[0-9a-fA-F]{8}$/.test(trimmed)) {
    return `#${trimmed.slice(0, 6).toLowerCase()}`
  }

  return fallback
}

/**
 * Checks whether is color field key.
 *
 * @param {*} key - Lookup key for the requested value.
 * @returns {boolean} Whether the condition is satisfied.
 */
export function isColorFieldKey(key) {
  return key === 'color' || key.endsWith('_color') || key.startsWith('color_')
}

/**
 * Normalizes color fields.
 *
 * @param {*} record - Value for record.
 * @returns {*} Derived data structure for downstream use.
 */
export function normalizeColorFields(record) {
  if (!record || typeof record !== 'object') {
    return record
  }

  return Object.fromEntries(Object.entries(record).map(([key, value]) => [key, isColorFieldKey(key) ? normalizeHexColor(value) : value]))
}
