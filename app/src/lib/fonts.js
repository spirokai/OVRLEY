/**
 * Provides shared fonts utilities for the app.
 */

export const RECOMMENDED_FONTS = [
  { id: 'Arial.ttf', name: 'Arial' },
  { id: 'Evogria.otf', name: 'Evogria' },
  { id: 'Furore.otf', name: 'Furore' },
]

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
  return RECOMMENDED_FONTS.find((font) => normalizeFontKey(font.id) === key || normalizeFontKey(font.name) === key) || null
}

/**
 * Returns font family name.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Requested value or structure.
 */
export function getFontFamilyName(value) {
  const recommendedFont = getRecommendedFont(value)
  if (recommendedFont) {
    return recommendedFont.name
  }

  return String(value || '').trim() || 'Arial'
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
  return trimmed.replace(/\.[^.]+$/, '') || 'Custom font'
}
