/**
 * Provides shared fonts utilities for the app.
 */

let bundledRecommendedFonts = []

const FONT_EXTENSION_PATTERN = /\.(ttf|otf|ttc|woff2?|fon)$/i

export function stripFontExtension(value) {
  const trimmed = String(value || '').trim()
  return trimmed.replace(FONT_EXTENSION_PATTERN, '')
}

export function setBundledRecommendedFonts(fonts) {
  bundledRecommendedFonts = Array.isArray(fonts) ? fonts : []
}

function getRecommendedFonts() {
  const byId = new Map()

  bundledRecommendedFonts.forEach((font) => {
    const id = String(font?.id || font?.name || '').trim()
    if (!id) {
      return
    }

    const option = {
      id,
      name: String(font?.name || stripFontExtension(id)).trim(),
    }

    const key = normalizeFontKey(option.id)
    if (!byId.has(key)) {
      byId.set(key, option)
    }
  })

  return [...byId.values()]
}
/**
 * Normalizes font key.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Derived data structure for downstream use.
 */
export function normalizeFontKey(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
}

/**
 * Returns recommended font.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Requested value or structure.
 */
function getRecommendedFont(value) {
  const key = normalizeFontKey(value)
  return getRecommendedFonts().find((font) => normalizeFontKey(font.id) === key || normalizeFontKey(font.name) === key) || null
}

function getFirstRecommendedFont() {
  return getRecommendedFonts()[0] || null
}

/**
 * Returns font family name.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Requested value or structure.
 */
export function getFontFamilyName(value) {
  if (!String(value || '').trim()) {
    const firstRecommendedFont = getFirstRecommendedFont()
    if (firstRecommendedFont) {
      return firstRecommendedFont.name
    }
  }

  const recommendedFont = getRecommendedFont(value)
  if (recommendedFont) {
    return recommendedFont.name
  }

  return stripFontExtension(value) || 'sans-serif'
}

/**
 * Creates font selection.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {object} Derived data structure for downstream use.
 */
export function createFontSelection(value) {
  return {
    font: value,
    font_family: getFontFamilyName(value),
  }
}

/**
 * Formats font label.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {string} Formatted representation of the input.
 */
export function formatFontLabel(value) {
  const trimmed = String(value || '').trim()
  return stripFontExtension(trimmed) || 'Custom font'
}
