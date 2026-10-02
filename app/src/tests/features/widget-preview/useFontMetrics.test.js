import { afterEach, expect, test, vi } from 'vitest'
import { getPreparedFont, prepareDocumentFonts, prepareFont } from '@/lib/font-resources'

const backend = vi.hoisted(() => ({
  listAvailableFonts: vi.fn(async () => ({ recommendedFonts: [], systemFonts: [] })),
  getFontCapabilities: vi.fn(async (id) => ({
    id,
    name: id,
    faces: [{ style: 'normal', weight: 400, axes: [], file: null, local_name: id }],
  })),
}))
vi.mock('@/api/backend', () => backend)
afterEach(() => vi.unstubAllGlobals())

test('prepares referenced families once, deduplicates concurrent requests, and retries failed preparation', async () => {
  const registered = []
  let fail = true
  const load = vi.fn(async function () {
    if (this.family === 'OVRLEY Retry' && fail) throw new Error('Font unavailable')
    return this
  })
  vi.stubGlobal(
    'FontFace',
    class {
      constructor(family) {
        this.family = family
      }
      load = load
    },
  )
  const originalFonts = Object.getOwnPropertyDescriptor(document, 'fonts')
  Object.defineProperty(document, 'fonts', { configurable: true, value: { add: (face) => registered.push(face) } })
  try {
    const document = {
      config: {
        labels: [
          { font: 'Arial', font_size: 30 },
          { font: 'Arial', font_size: 60 },
        ],
        values: [{ font: 'Evogria' }],
      },
    }
    await Promise.all([prepareDocumentFonts(document), prepareDocumentFonts(document)])
    expect(load).toHaveBeenCalledTimes(2)
    expect(backend.getFontCapabilities).toHaveBeenCalledTimes(2)
    await prepareDocumentFonts({ ...document, previewSecond: 9 })
    expect(load).toHaveBeenCalledTimes(2)
    await expect(prepareFont('Retry')).rejects.toThrow('Font unavailable')
    expect(() => getPreparedFont('Retry')).toThrow('has not been prepared')
    expect(registered).toHaveLength(2)
    fail = false
    expect(await prepareFont('Retry')).toBe(getPreparedFont('Retry'))
    expect(registered).toHaveLength(3)
  } finally {
    if (originalFonts) Object.defineProperty(globalThis.document, 'fonts', originalFonts)
    else delete globalThis.document.fonts
  }
})
