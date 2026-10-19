/**
 * Font selections, catalog presentation, and supported style matching.
 */

const FONT_EXTENSION_PATTERN = /\.(ttf|otf|ttc|woff2?|fon)$/i

/** @param {string} value Font identity or display name. @returns {string} Name without a font-file extension. */
export function getFontFamilyName(value) {
  return value.trim().replace(FONT_EXTENSION_PATTERN, '')
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
 * @param {string} value Selected font ID.
 * @param {object[]} recommendedFonts Bundled families.
 * @param {object[]} systemFonts System families.
 * @returns {object} Catalog options, preserving an unavailable saved selection.
 */
export function getFontSelectOptions(value, recommendedFonts, systemFonts) {
  let known = !value
  const names = new Set()
  for (const font of recommendedFonts) {
    if (font.id === value) known = true
    names.add(font.name)
  }
  const filteredSystemFonts = []
  for (const font of systemFonts) {
    if (font.id === value) known = true
    if (!names.has(font.name)) filteredSystemFonts.push(font)
  }
  return {
    recommendedOptions: known ? recommendedFonts : [{ id: value, name: getFontFamilyName(value) || 'Custom font' }, ...recommendedFonts],
    filteredSystemFonts,
  }
}

/** @param {object} face Resolved font face. @returns {object|undefined} Visible weight axis. */
export function getFontWeightAxis(face) {
  for (const axis of face.axes) {
    if (axis.tag === 'wght' && !axis.hidden) return axis
  }
}

/** @param {object} face Face metadata. @returns {boolean} Whether a variation axis can slant glyphs. */
export function hasItalicVariation(face) {
  for (const axis of face.axes) {
    if ((axis.tag === 'ital' && axis.max > 0) || (axis.tag === 'slnt' && (axis.min < 0 || axis.max > 0))) return true
  }
  return false
}

/** @param {object} face Font face metadata. @returns {boolean} Italic/slant support, including OS-simulated faces. */
function supportsItalicFace(face) {
  return face.style === 'italic' || face.style === 'oblique' || hasItalicVariation(face)
}

/** @param {object} font Resolved family capabilities. @returns {boolean} Available italic/slant support. */
export function supportsFontItalic(font) {
  return font.faces.some(supportsItalicFace)
}

/** @param {object} font Capabilities. @param {boolean} italic Requested style. @returns {object[]} Faces eligible for style matching. */
export function styleFaces(font, italic) {
  const slanted = italic && supportsFontItalic(font)
  const candidates = []
  const italics = []
  for (const face of font.faces) {
    if (!(slanted ? supportsItalicFace(face) : face.style === 'normal')) continue
    candidates.push(face)
    if (slanted && (face.style === 'italic' || face.axes.some(isItalicAxis))) italics.push(face)
  }
  if (slanted) {
    // Prefer an italic face/ital axis to an oblique face/slnt axis, then match weight.
    if (italics.length) return italics
  }
  return candidates
}

function isItalicAxis(axis) {
  return axis.tag === 'ital' && axis.max > 0
}

function isWeightAxis(axis) {
  return axis.tag === 'wght'
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
 * @param {boolean} italic Requested italic state; unsupported families stay upright.
 * @returns {{ face: object, weight: number }} Supported face and weight.
 */
function resolveFontStyle(font, requested, italic) {
  const candidates = []
  for (const face of styleFaces(font, italic)) {
    const axis = face.axes.find(isWeightAxis)
    const weight = axis ? (axis.hidden ? axis.default : Math.min(axis.max, Math.max(axis.min, requested))) : face.weight
    candidates.push({ face, weight, rank: weightRank(requested, weight) })
  }
  if (!candidates.length) throw new Error(`${font.id}: no matching font face`)
  candidates.sort(compareWeightRank)
  return { face: candidates[0].face, weight: candidates[0].weight }
}

function compareWeightRank(a, b) {
  return a.rank[0] - b.rank[0] || a.rank[1] - b.rank[1]
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

/** @param {object} value Config or widget updates. @returns {string[]} Font IDs, including inactive variants. */
export function collectFontIds(value) {
  const ids = new Set()
  collectFontIdsFromRecord(value, ids)
  return [...ids]
}

function collectFontIdsFromRecord(record, ids) {
  for (const [key, item] of Object.entries(record)) {
    if (FONT_KEYS.has(key) && item !== undefined && item !== null) ids.add(item)
    else if (item !== null && typeof item === 'object') collectFontIdsFromRecord(item, ids)
  }
}
