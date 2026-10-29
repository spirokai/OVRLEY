import { afterEach, expect, test, vi } from 'vite-plus/test'
import { createLabelFontSelection, supportsFontItalic } from '@/lib/fonts'

import { prepareFont } from '@/lib/font-resources'

const catalog = vi.hoisted(() => ({ recommendedFonts: [], systemFonts: [] }))
vi.mock('@/api/backend', () => ({ listAvailableFonts: async () => catalog }))

afterEach(() => vi.unstubAllGlobals())

test('registers genuine ital and slnt instances while weight-only fonts remain upright', async () => {
  const weight = { tag: 'wght', min: 100, default: 400, max: 900, hidden: false }
  const axisFonts = [
    { id: 'Axis italic', tag: 'ital', min: 0, max: 1, expected: '"ital" 1' },
    { id: 'Axis slant', tag: 'slnt', min: -10, max: 0, expected: '"slnt" -10' },
  ]
  const fonts = axisFonts.map(({ id, tag, min, max }) => ({
    id,
    name: id,
    faces: [{ style: 'normal', weight: 400, axes: [weight, { tag, min, default: 0, max, hidden: false }], file: null, local_name: id }],
  }))
  const upright = {
    id: 'Weight only',
    name: 'Weight only',
    faces: [{ style: 'normal', weight: 400, axes: [weight], file: null, local_name: 'Upright' }],
  }
  catalog.recommendedFonts = [...fonts, upright]
  const registered = []
  vi.stubGlobal(
    'FontFace',
    class {
      constructor(family, source, descriptors) {
        Object.assign(this, { family, source, ...descriptors })
      }
      async load() {
        return this
      }
    },
  )
  const originalFonts = Object.getOwnPropertyDescriptor(document, 'fonts')
  Object.defineProperty(document, 'fonts', { configurable: true, value: { add: (face) => registered.push(face) } })
  try {
    for (const [index, font] of fonts.entries()) {
      expect(supportsFontItalic(font)).toBe(true)
      await prepareFont(font.id)
      expect(createLabelFontSelection(font, { font_weight: 537, italic: true })).toMatchObject({ font_weight: 537, italic: true })
      const faces = registered.filter((face) => face.family === `OVRLEY ${font.id}`)
      expect(faces.map((face) => face.style)).toEqual(['normal', 'italic'])
      expect(faces[0].variationSettings).toBe(`"${axisFonts[index].tag}" 0`)
      expect(faces[1].variationSettings).toBe(axisFonts[index].expected)
      expect(faces[1].weight).toBe('100 900')
    }
    expect(supportsFontItalic(upright)).toBe(false)
    expect(createLabelFontSelection(upright, { font_weight: 537, italic: true })).toMatchObject({ font_weight: 537, italic: false })
  } finally {
    if (originalFonts) Object.defineProperty(document, 'fonts', originalFonts)
    else delete document.fonts
  }
})
