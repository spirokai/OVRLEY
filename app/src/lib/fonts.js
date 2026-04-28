export const RECOMMENDED_FONTS = [
  { id: 'Arial.ttf', name: 'Arial' },
  { id: 'Evogria.otf', name: 'Evogria' },
  { id: 'Furore.otf', name: 'Furore' },
]

export function normalizeFontKey(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
}

export function getRecommendedFont(value) {
  const key = normalizeFontKey(value)
  return (
    RECOMMENDED_FONTS.find(
      (font) =>
        normalizeFontKey(font.id) === key ||
        normalizeFontKey(font.name) === key,
    ) || null
  )
}

export function getFontFamilyName(value) {
  const recommendedFont = getRecommendedFont(value)
  if (recommendedFont) {
    return recommendedFont.name
  }

  return String(value || '').trim() || 'Arial'
}

export function createFontSelection(value) {
  return {
    font: value,
    font_family: getFontFamilyName(value),
  }
}

export function formatFontLabel(value) {
  const trimmed = String(value || '').trim()
  return trimmed.replace(/\.[^.]+$/, '') || 'Custom font'
}
