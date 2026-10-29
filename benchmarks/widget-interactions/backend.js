/** Fixture transport replaces only native resource loading in the browser benchmark. */

async function resource(path) {
  const response = await fetch(path)
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`)
  return response.arrayBuffer()
}

/** @param {string} id Fixture activity identity. @returns {Promise<ArrayBuffer>} Original parsed activity bytes. */
export function previewActivity(id) {
  if (id !== 'interaction-benchmark') throw new Error(`Unknown benchmark activity: ${id}`)
  return resource('/data/activity.json')
}

/** @param {object} request Production font resource request. @returns {Promise<ArrayBuffer>} Native serialized typeface. */
export async function previewFont(request) {
  const response = await fetch('/data/fonts.json')
  if (!response.ok) throw new Error(`fonts.json: HTTP ${response.status}`)
  const fonts = await response.json()
  const font = fonts.find((entry) => JSON.stringify(entry.request) === JSON.stringify(request))
  if (!font) throw new Error(`Unprepared benchmark font: ${JSON.stringify(request)}`)
  return resource(`/data/${font.path}`)
}

/** @returns {never} Raster resources are outside these two workloads. */
export function rasterPreviewPng() {
  throw new Error('The interaction benchmark has no raster resources')
}
