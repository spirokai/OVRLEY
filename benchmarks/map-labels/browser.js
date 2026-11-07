// Map label benchmark, WASM side: the production `wasm-map-bindings` on WebGL2, called as the editor
// preview calls it. Decoding runs on this thread rather than in tile workers, so it can be timed.
import init, { MapLayer, TileDecoder } from '/wasm/wasm_map_bindings.js'

let ready
const now = () => performance.now()
const json = async (path) => (await fetch(path)).json()
const bytes = async (path) => {
  const response = await fetch(path)
  if (!response.ok) throw new Error(`${path}: HTTP ${response.status}`)
  return new Uint8Array(await response.arrayBuffer())
}

/** Runs every case of the plan, labels off then on, and returns the samples. */
export async function play() {
  ready ??= init({ module_or_path: '/wasm/wasm_map_bindings_bg.wasm' })
  await ready
  const plan = await json('/plan.json')
  const style = await bytes('/style.json')
  const tiles = new Map()
  for (const [z, x, y] of plan.tiles) tiles.set(`${z}/${x}/${y}`, await bytes(`/tiles/${z}/${x}/${y}.pbf`))
  const results = []
  let renderer = ''
  for (const spec of plan.cases)
    for (const labels of [false, true]) {
      console.log(`[wasm] ${spec.id} labels=${labels}`)
      const result = await runCase(plan, spec, labels, style, tiles)
      renderer = result.renderer
      delete result.renderer
      results.push(result)
    }
  return { backend: 'wasm', renderer, user_agent: navigator.userAgent, cross_origin_isolated: crossOriginIsolated, results }
}

async function runCase(plan, spec, labels, style, tiles) {
  const config = JSON.stringify({
    style: plan.style,
    style_buildings: plan.style_buildings,
    buildings: true,
    labels,
    paths: true,
    width: spec.size,
    height: spec.size,
    pixel_ratio: spec.pixel_ratio,
    msaa_samples: 4,
    zoom: spec.zoom,
  })

  // What a tile worker does when it starts: fetch the fonts the style needs and build its decoder.
  let started = now()
  const fontNames = TileDecoder.font_files(config, style)
  const fonts = await Promise.all(fontNames.map((name) => bytes(`/fonts/${name}`)))
  const fontFetchMs = now() - started
  started = now()
  const decoder = new TileDecoder(config, style, fonts)
  const decoderNewMs = now() - started

  const canvas = document.createElement('canvas')
  canvas.width = spec.size
  canvas.height = spec.size
  document.body.append(canvas)
  started = now()
  const layer = await MapLayer.create(canvas, config, style)
  const layerCreateMs = now() - started
  const gl = canvas.getContext('webgl2')
  const debug = gl.getExtension('WEBGL_debug_renderer_info')
  const renderer = debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER)
  if (spec.route) layer.set_route(Float64Array.from(spec.route.flat()))
  const routeStyle = Float32Array.from(spec.route_style.map((value, index) => (index === 8 ? value * spec.pixel_ratio : value)))

  // Every tile the case needs, decoded twice: the first pass rasterizes glyphs, the second finds them made.
  const frames = spec.frames.map((frame) => ({
    camera: Float64Array.from(frame.camera),
    labels: labels ? Float64Array.from(frame.labels) : new Float64Array(0),
    travelled: frame.travelled_metres,
  }))
  const needed = new Set()
  for (const frame of frames) {
    const missing = layer.missing(frame.camera, frame.labels)
    for (let index = 0; index < missing.length; index += 3) needed.add(`${missing[index]}/${missing[index + 1]}/${missing[index + 2]}`)
  }
  const decoded = new Map()
  started = now()
  for (const key of needed) decoded.set(key, decoder.decode(tiles.get(key)))
  const decodeColdMs = now() - started
  started = now()
  for (const key of needed) decoder.decode(tiles.get(key))
  const decodeWarmMs = now() - started
  const decodedBytes = [...decoded.values()].reduce((sum, tile) => sum + tile.length, 0)
  decoder.free()

  const upload = (frame) => {
    const missing = layer.missing(frame.camera, frame.labels)
    for (let index = 0; index < missing.length; index += 3)
      layer.upload(
        missing[index],
        missing[index + 1],
        missing[index + 2],
        decoded.get(`${missing[index]}/${missing[index + 1]}/${missing[index + 2]}`),
      )
    return missing.length / 3
  }
  started = now()
  let uploadedTiles = 0
  // A pool that grows drops its tiles, so repeat until the first view is resident.
  for (let added = upload(frames[0]); added > 0; added = upload(frames[0])) uploadedTiles += added
  const firstUploadMs = now() - started

  // Reading one pixel of the canvas waits for the GPU to finish the frame.
  const pixel = new Uint8Array(4)
  const sync = () => {
    const previous = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING)
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, null)
    gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel)
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previous)
  }
  const uploadMs = [],
    drawMs = [],
    frameMs = []
  for (const [index, frame] of frames.entries()) {
    let started = now()
    upload(frame)
    const uploaded = now() - started
    started = now()
    if (spec.route) layer.draw_with_route(frame.camera, frame.labels, routeStyle, frame.travelled, 0)
    else layer.draw(frame.camera, frame.labels, 0)
    const drawn = now() - started
    sync()
    const finished = now() - started
    if (index >= plan.warmup) {
      uploadMs.push(uploaded)
      drawMs.push(drawn)
      frameMs.push(finished)
    }
    // Lets the browser composite, as the preview's animation frames do.
    if (index % 30 === 29) await new Promise((resolve) => setTimeout(resolve, 0))
  }
  layer.free()
  gl.getExtension('WEBGL_lose_context')?.loseContext()
  canvas.remove()
  return {
    case: spec.id,
    area: spec.area,
    size: spec.size,
    camera: spec.camera,
    labels,
    tiles: needed.size,
    renderer,
    prepare: {
      font_files: fontNames.length,
      font_bytes: fonts.reduce((sum, font) => sum + font.length, 0),
      font_fetch_ms: fontFetchMs,
      decoder_new_ms: decoderNewMs,
      decode_cold_ms: decodeColdMs,
      decode_warm_ms: decodeWarmMs,
      decoded_bytes: decodedBytes,
      layer_create_ms: layerCreateMs,
      first_upload_ms: firstUploadMs,
      first_upload_tiles: uploadedTiles,
    },
    frames: { upload_ms: uploadMs, draw_ms: drawMs, frame_ms: frameMs },
  }
}
