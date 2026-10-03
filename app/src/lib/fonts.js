/**
 * Provides shared fonts utilities for the app.
 */

const FONT_EXTENSION_PATTERN = /\.(ttf|otf|ttc|woff2?|fon)$/i

/** @param {string} value Font identity or display name. @returns {string} Name without a font-file extension. */
export function stripFontExtension(value) {
  const trimmed = value.trim()
  return trimmed.replace(FONT_EXTENSION_PATTERN, '')
}

/**
 * Returns font family name.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Requested value or structure.
 */
export function getFontFamilyName(value) {
  return stripFontExtension(value)
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

/**
 * @param {string} value Selected font ID.
 * @param {object[]} recommendedFonts Bundled families.
 * @param {object[]} systemFonts System families.
 * @returns {object} Catalog options, preserving an unavailable saved selection.
 */
export function getFontSelectOptions(value, recommendedFonts, systemFonts) {
  const known = !value || [...recommendedFonts, ...systemFonts].some((font) => font.id === value)
  const names = new Set(recommendedFonts.map((font) => font.name))
  return {
    recommendedOptions: known ? recommendedFonts : [{ id: value, name: formatFontLabel(value) }, ...recommendedFonts],
    filteredSystemFonts: systemFonts.filter((font) => !names.has(font.name)),
  }
}

/** @param {object} face Resolved font face. @returns {object|undefined} Visible weight axis. */
export function getFontWeightAxis(face) {
  return face.axes.find((axis) => axis.tag === 'wght' && !axis.hidden)
}

/** @param {object} face Face metadata. @returns {boolean} Whether a variation axis can slant glyphs. */
export function hasItalicVariation(face) {
  return face.axes.some((axis) => (axis.tag === 'ital' && axis.max > 0) || (axis.tag === 'slnt' && (axis.min < 0 || axis.max > 0)))
}

/** @param {object} face Font face metadata. @returns {boolean} Italic/slant support, including OS-simulated faces. */
export function supportsItalicFace(face) {
  return face.style === 'italic' || face.style === 'oblique' || hasItalicVariation(face)
}

/** @param {object} font Resolved family capabilities. @returns {boolean} Available italic/slant support. */
export function supportsFontItalic(font) {
  return font.faces.some(supportsItalicFace)
}

/** @param {object} font Capabilities. @param {boolean} italic Requested style. @returns {object[]} Faces eligible for style matching. */
export function styleFaces(font, italic) {
  const slanted = italic && supportsFontItalic(font)
  const candidates = font.faces.filter((face) => (slanted ? supportsItalicFace(face) : face.style === 'normal'))
  if (slanted) {
    // Prefer an italic face/ital axis to an oblique face/slnt axis, then match weight.
    const italics = candidates.filter((face) => face.style === 'italic' || face.axes.some((axis) => axis.tag === 'ital' && axis.max > 0))
    if (italics.length) return italics
  }
  return candidates
}

function weightRank(requested, candidate) {
  if (requested < 400) return candidate <= requested ? [0, requested - candidate] : [1, candidate - requested]
  if (requested <= 500) {
    if (candidate >= requested && candidate <= 500) return [0, candidate - requested]
    return candidate < requested ? [1, requested - candidate] : [2, candidate - requested]
  }
  return candidate >= requested ? [0, candidate - requested] : [1, requested - candidate]
}

/**
 * CSS Fonts 4 face matching, shared with Rust's font-resolution policy.
 * @param {object} font Resolved family capabilities.
 * @param {number} requested Validated requested weight.
 * @param {boolean} [italic=false] Requested italic state; unsupported families stay upright.
 * @returns {{ face: object, weight: number, fontStyle: string }} Supported style and weight.
 */
export function resolveFontStyle(font, requested, italic = false) {
  const candidates = styleFaces(font, italic).map((face) => {
    const axis = face.axes.find((axis) => axis.tag === 'wght')
    const weight = axis ? (axis.hidden ? axis.default : Math.min(axis.max, Math.max(axis.min, requested))) : face.weight
    return { face, weight, rank: weightRank(requested, weight) }
  })
  if (!candidates.length) throw new Error(`${font.id}: no matching font face`)
  candidates.sort((a, b) => a.rank[0] - b.rank[0] || a.rank[1] - b.rank[1])
  return { face: candidates[0].face, weight: candidates[0].weight, fontStyle: italic && supportsFontItalic(font) ? 'italic' : 'normal' }
}

/** @param {object} font Capabilities. @param {number} requested Weight. @param {boolean} [italic=false] Style. @returns {object} Weight controls. */
export function getFontWeightControl(font, requested, italic = false) {
  const matched = resolveFontStyle(font, requested, italic)
  return {
    weightAxis: getFontWeightAxis(matched.face),
    weight: matched.weight,
  }
}

/** @param {object} font Prepared capabilities. @param {object} label Current label. @returns {object} Supported font selection. */
export function createLabelFontSelection(font, label) {
  const italic = label.italic && supportsFontItalic(font)
  return { ...createFontSelection(font.id), italic, font_weight: resolveFontStyle(font, label.font_weight, italic).weight }
}

const FONT_KEYS = new Set(['font', 'label_font', 'min_max_label_font', 'font_text', 'font_values'])

/** @param {object} value Config or widget updates. @param {object} [previous] When given, collects changed identities only. @returns {string[]} Font IDs, including inactive variants. */
export function collectFontIds(value, previous) {
  const ids = new Set()
  function visit(record, previous) {
    for (const [key, item] of Object.entries(record)) {
      if (FONT_KEYS.has(key) && item !== undefined && item !== null && item !== previous?.[key]) ids.add(item)
      else if (item !== null && typeof item === 'object') visit(item, previous?.[key])
    }
  }
  visit(value, previous)
  return [...ids]
}
