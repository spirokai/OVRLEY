/**
 * Provides shared fonts utilities for the app.
 */

const fontFamilies = new Map()
const fontRequests = new Map()

const FONT_EXTENSION_PATTERN = /\.(ttf|otf|ttc|woff2?|fon)$/i

export function stripFontExtension(value) {
  const trimmed = String(value || '').trim()
  return trimmed.replace(FONT_EXTENSION_PATTERN, '')
}

/** @param {object} catalog Canonical backend catalog. @returns {void} */
export function setFontCatalog(catalog) {
  for (const font of [...catalog.recommendedFonts, ...catalog.systemFonts]) fontFamilies.set(font.id, font)
}
/**
 * Returns font family name.
 *
 * @param {*} value - Input value processed by the helper.
 * @returns {*} Requested value or structure.
 */
export function getFontFamilyName(value) {
  return fontFamilies.get(value)?.name ?? stripFontExtension(value)
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

function hasItalicVariation(face) {
  return face.axes.some((axis) => (axis.tag === 'ital' && axis.max > 0) || (axis.tag === 'slnt' && (axis.min < 0 || axis.max > 0)))
}

/** @param {object} face Font face metadata. @returns {boolean} Italic/slant support, including OS-simulated faces. */
export function supportsItalicFace(face) {
  return face.style === 'italic' || face.style === 'oblique' || hasItalicVariation(face)
}

function isSimulatedItalicFace(font, face) {
  // Windows exposes simulated slants with the upright face's exact local name.
  // Registering those regular glyphs as italic would prevent browser synthesis.
  return (
    face.file === null &&
    face.style !== 'normal' &&
    !hasItalicVariation(face) &&
    font.faces.some((upright) => upright.style === 'normal' && upright.local_name === face.local_name)
  )
}

/** @param {object} font Resolved family capabilities. @returns {boolean} Available italic/slant support. */
export function supportsFontItalic(font) {
  return font.faces.some(supportsItalicFace)
}

function styleFaces(font, italic) {
  const slanted = italic && supportsFontItalic(font)
  const candidates = font.faces.filter((face) => (slanted ? supportsItalicFace(face) : face.style === 'normal'))
  if (slanted) {
    // Prefer an italic face/ital axis to an oblique face/slnt axis, then match weight.
    const italics = candidates.filter((face) => face.style === 'italic' || face.axes.some((axis) => axis.tag === 'ital' && axis.max > 0))
    if (italics.length) return italics
  }
  return candidates
}

function styleVariations(face, italic) {
  return (
    face.axes
      .filter((axis) => axis.tag !== 'wght')
      .map((axis) => {
        let value = axis.default
        if (axis.tag === 'ital') value = italic ? Math.min(axis.max, Math.max(axis.min, 1)) : 0
        if (axis.tag === 'slnt') value = italic ? Math.min(axis.max, Math.max(axis.min, axis.min < 0 ? -12 : 12)) : 0
        return `"${axis.tag}" ${value}`
      })
      .join(', ') || 'normal'
  )
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

/**
 * Uses the same supported-face matching as Rust. While discovery is pending,
 * the requested weight is used until font readiness refreshes measurements.
 * @param {string} id Canonical font ID.
 * @param {number} requested Validated weight.
 * @param {boolean} [italic=false] Requested italic state.
 * @returns {{ weight: number, fontStyle: string }} Supported rendering style.
 */
export function getFontRenderStyle(id, requested, italic = false) {
  const font = fontFamilies.get(id)
  return font?.faces ? resolveFontStyle(font, requested, italic) : { weight: requested, fontStyle: 'normal' }
}

/** @param {object} font Capabilities. @param {number} requested Weight. @param {boolean} [italic=false] Style. @returns {object} Weight controls. */
export function getFontWeightControl(font, requested, italic = false) {
  const matched = resolveFontStyle(font, requested, italic)
  return {
    weightAxis: getFontWeightAxis(matched.face),
    weight: matched.weight,
    weightOptions: [...new Set(styleFaces(font, italic).map((face) => face.weight))]
      .sort((a, b) => a - b)
      .map((weight) => ({ value: String(weight), label: String(weight) })),
  }
}

/** @param {object} value Cloned saved input. @returns {void} Migrates changed font identities in place. */
export function migrateSavedFontIdentities(value) {
  for (const [key, item] of Object.entries(value)) {
    if (
      ['font', 'label_font', 'min_max_label_font', 'font_text', 'font_values'].includes(key) &&
      ['Inter ExtraBold.ttf', 'Inter ExtraBold'].includes(item)
    ) {
      value[key] = 'Inter.ttf'
    } else if (key === 'font_family' && item === 'Inter ExtraBold') {
      value[key] = 'Inter'
    } else if (item !== null && typeof item === 'object') {
      migrateSavedFontIdentities(item)
    }
  }
}

/**
 * Registers physical faces and ital/slnt instances discovered by Skia. Windows
 * simulated slants use browser style synthesis; other axes keep defaults.
 * @param {string} id Canonical font ID.
 * @returns {Promise<object>} Registered family capabilities.
 */
export function loadFont(id) {
  if (!fontRequests.has(id)) {
    const request = (async () => {
      const backend = await import('@/api/backend')
      await backend.listAvailableFonts()
      let font = fontFamilies.get(id)
      if (!font || font.faces === null) {
        font = await backend.getFontCapabilities(id)
        fontFamilies.set(id, font)
      }
      if (typeof FontFace !== 'undefined' && document.fonts) {
        const italicFaces = supportsFontItalic(font) ? styleFaces(font, true) : []
        await Promise.all(
          font.faces.map(async (face, index) => {
            if (isSimulatedItalicFace(font, face)) return
            const axis = getFontWeightAxis(face)
            const source = face.file === null ? `local(${JSON.stringify(face.local_name)})` : new Uint8Array(await backend.getFontData(id, index))
            const styles = [...(face.style === 'normal' ? [false] : []), ...(italicFaces.includes(face) ? [true] : [])]
            await Promise.all(
              styles.map(async (italic) => {
                const registered = new FontFace(`OVRLEY ${id}`, source, {
                  style: italic ? 'italic' : 'normal',
                  weight: axis ? `${axis.min} ${axis.max}` : String(face.weight),
                  variationSettings: styleVariations(face, italic),
                })
                document.fonts.add(await registered.load())
              }),
            )
          }),
        )
      }
      return font
    })()
    fontRequests.set(id, request)
  }
  return fontRequests.get(id)
}
