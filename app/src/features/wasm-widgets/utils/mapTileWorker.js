// One tile worker: fetches a tile through the loopback cache, then decodes, tessellates and labels it.
// Labels need the bundled font files, which the shell serves beside the tiles; the decoder names them.
const BASE_URL = '/wasm-bindings'
let ready
let tileUrl

async function start({ config, style, tileUrl: url }) {
  tileUrl = url
  const module = await import(/* @vite-ignore */ `${BASE_URL}/wasm_map_bindings.js`)
  await module.default({ module_or_path: `${BASE_URL}/wasm_map_bindings_bg.wasm` })
  const fonts = await Promise.all(module.TileDecoder.font_files(config, style).map(fetchFont))
  return new module.TileDecoder(config, style, fonts)
}

/** @param {string} name Font file name. @returns {Promise<Uint8Array>} Its bytes. */
async function fetchFont(name) {
  const response = await fetch(new URL(`/fonts/${name}`, tileUrl))
  if (!response.ok) throw new Error(`Map font ${name}: HTTP ${response.status}`)
  return new Uint8Array(await response.arrayBuffer())
}

async function decode(decoder, { z, x, y }) {
  const url = tileUrl.replace('{z}', z).replace('{x}', x).replace('{y}', y)
  try {
    const response = await fetch(url)
    if (!response.ok) throw new Error(`HTTP ${response.status}`)
    const tile = decoder.decode(new Uint8Array(await response.arrayBuffer()))
    self.postMessage({ z, x, y, tile }, [tile.buffer])
  } catch (error) {
    self.postMessage({ z, x, y, error: String(error?.message ?? error) })
  }
}

self.onmessage = ({ data }) => {
  if (data.config) {
    ready = start(data)
    ready.catch((error) => self.postMessage({ fatal: String(error?.message ?? error) }))
    return
  }
  ready.then((decoder) => decode(decoder, data))
}
