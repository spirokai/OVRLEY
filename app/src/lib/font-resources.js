import { getFontCapabilities, getFontData, listAvailableFonts } from '@/api/backend'
import { createCachedPromise } from './cached-promise'
import { collectFontIds, getFontWeightAxis, hasItalicVariation, styleFaces, supportsFontItalic } from './fonts'

const preparations = new Map()
const preparedFonts = new Map()

/** @returns {Promise<object>} Session catalog; failed requests can be retried. */
export const getFontCatalog = createCachedPromise(() => listAvailableFonts())

/** @param {string} id Font identity. @returns {object} Capabilities of a font already prepared by the document/edit owner. */
export function getPreparedFont(id) {
  const font = preparedFonts.get(id)
  if (!font) throw new Error(`Font has not been prepared: ${id}`)
  return font
}

/** @param {string} id Font identity. @returns {boolean} Whether its browser registrations are complete. */
export function isFontPrepared(id) {
  return preparedFonts.has(id)
}

/** @param {string} id Font identity. @returns {Promise<object>} Deduplicated, retryable family preparation, independent of text and playback. */
export function prepareFont(id) {
  if (!preparations.has(id)) {
    preparations.set(
      id,
      createCachedPromise(async () => {
        const catalog = await getFontCatalog()
        const listed = [...catalog.recommendedFonts, ...catalog.systemFonts].find((font) => font.id === id)
        const font = listed?.faces ? listed : await getFontCapabilities(id)
        await registerBrowserFont(font)
        preparedFonts.set(id, font)
        return font
      }),
    )
  }
  return preparations.get(id)()
}

/** @param {object} value Config, widget data, or document containing global defaults. @returns {Promise<void>} Prepares every referenced family. */
export async function prepareDocumentFonts(value) {
  await Promise.all(collectFontIds(value).map(prepareFont))
}

/** @param {object} font Backend family capabilities. @returns {Promise<void>} Registers all physical styles and variable instances together. */
async function registerBrowserFont(font) {
  const italicFaces = supportsFontItalic(font) ? styleFaces(font, true) : []
  const faces = await Promise.all(
    font.faces.map(async (face, index) => {
      // Windows reports synthetic slants using the upright local identity.
      // Keep the upright registration so the browser can synthesize that slant.
      if (isSimulatedItalicFace(font, face)) return []
      const axis = getFontWeightAxis(face)
      const source = face.file === null ? `local(${JSON.stringify(face.local_name)})` : new Uint8Array(await getFontData(font.id, index))
      const styles = [...(face.style === 'normal' ? [false] : []), ...(italicFaces.includes(face) ? [true] : [])]
      return Promise.all(
        styles.map((italic) =>
          new FontFace(`OVRLEY ${font.id}`, source, {
            style: italic ? 'italic' : 'normal',
            weight: axis ? `${axis.min} ${axis.max}` : String(face.weight),
            variationSettings: styleVariations(face, italic),
          }).load(),
        ),
      )
    }),
  )
  for (const face of faces.flat()) document.fonts.add(face)
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
