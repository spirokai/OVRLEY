// WebGL diagnosis: what the production map module asks of WebGL per frame, and where the time goes.
import init, { MapLayer, TileDecoder } from '/wasm/wasm_map_bindings.js'

const now = () => performance.now()
const bytes = async (path) => new Uint8Array(await (await fetch(path)).arrayBuffer())

// Every WebGL2 call, counted and timed on this thread. Installed before wgpu creates its context.
const census = new Map()
let counting = false
const proto = WebGL2RenderingContext.prototype
for (const name of Object.getOwnPropertyNames(proto)) {
  const descriptor = Object.getOwnPropertyDescriptor(proto, name)
  if (typeof descriptor.value !== 'function' || name === 'constructor') continue
  const original = descriptor.value
  proto[name] = function (...args) {
    if (!counting) return original.apply(this, args)
    const started = now()
    const result = original.apply(this, args)
    const entry = census.get(name) ?? { calls: 0, ms: 0, bytes: 0 }
    entry.calls++
    entry.ms += now() - started
    if (name === 'bufferSubData' || name === 'bufferData') entry.bytes += args[2]?.byteLength ?? args[1]?.byteLength ?? 0
    if (name === 'drawElementsInstanced') {
      entry.bytes += args[1]
      entry.largest = Math.max(entry.largest ?? 0, args[1])
    }
    if (name === 'texSubImage2D') entry.bytes += args.find((arg) => arg?.byteLength)?.byteLength ?? 0
    census.set(name, entry)
    return result
  }
}

let ready
/** Runs one case variant: `frames` measured frames after `warmup`, with the census on for the measured ones. */
export async function diagnose({ caseId, labels, msaa, buildings, warmup, frames, census: withCensus, snapshot: withSnapshot }) {
  ready ??= init({ module_or_path: '/wasm/wasm_map_bindings_bg.wasm' })
  await ready
  const plan = await (await fetch('/plan.json')).json()
  const spec = plan.cases.find((entry) => entry.id === caseId)
  const style = await bytes('/style.json')
  const config = JSON.stringify({
    style: plan.style,
    style_buildings: plan.style_buildings,
    buildings,
    labels,
    paths: true,
    local_names: false,
    width: spec.size,
    height: spec.size,
    pixel_ratio: spec.pixel_ratio,
    msaa_samples: msaa,
    zoom: spec.zoom,
  })
  const fonts = await Promise.all(TileDecoder.font_files(config, style).map((name) => bytes(`/fonts/${name}`)))
  const decoder = new TileDecoder(config, style, fonts)
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = spec.size
  document.body.append(canvas)
  const layer = await MapLayer.create(canvas, config, style)
  const gl = canvas.getContext('webgl2')
  if (spec.route) layer.set_route(Float64Array.from(spec.route.flat()))
  const routeStyle = Float32Array.from(spec.route_style)
  const list = spec.frames.slice(0, warmup + frames).map((frame) => ({
    camera: Float64Array.from(frame.camera),
    labels: labels ? Float64Array.from(frame.labels) : new Float64Array(0),
    travelled: frame.travelled_metres,
  }))
  const decoded = new Map()
  const upload = async (frame) => {
    let missing = layer.missing(frame.camera, frame.labels)
    while (missing.length) {
      for (let index = 0; index < missing.length; index += 3) {
        const key = `${missing[index]}/${missing[index + 1]}/${missing[index + 2]}`
        if (!decoded.has(key)) decoded.set(key, decoder.decode(await bytes(`/tiles/${key}.pbf`)))
        layer.upload(missing[index], missing[index + 1], missing[index + 2], decoded.get(key))
      }
      missing = layer.missing(frame.camera, frame.labels)
    }
  }
  const pixel = new Uint8Array(4)
  const sync = () => {
    const previous = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING)
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, null)
    gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel)
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previous)
  }
  const draw = (frame) =>
    spec.route ? layer.draw_with_route(frame.camera, frame.labels, routeStyle, frame.travelled, 0) : layer.draw(frame.camera, frame.labels, 0)

  census.clear()
  const drawMs = [],
    syncMs = [],
    idleSyncMs = []
  for (const [index, frame] of list.entries()) {
    await upload(frame)
    const measured = index >= warmup
    counting = measured && withCensus
    let started = now()
    draw(frame)
    const drawn = now() - started
    counting = false
    started = now()
    sync()
    const synced = now() - started
    // A second readback with nothing queued: the round trip to the GPU process alone.
    started = now()
    sync()
    const idle = now() - started
    if (measured) {
      drawMs.push(drawn)
      syncMs.push(synced)
      idleSyncMs.push(idle)
    }
    if (index % 30 === 29) await new Promise((resolve) => setTimeout(resolve, 0))
  }
  // The last frame drawn again and read back whole, to compare builds.
  let snapshot = null
  if (withSnapshot) {
    draw(list[list.length - 1])
    const pixels = new Uint8Array(spec.size * spec.size * 4)
    const previous = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING)
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, null)
    gl.readPixels(0, 0, spec.size, spec.size, gl.RGBA, gl.UNSIGNED_BYTE, pixels)
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previous)
    snapshot = btoa(Array.from(pixels, (byte) => String.fromCharCode(byte)).join(''))
  }
  layer.free()
  decoder.free()
  gl.getExtension('WEBGL_lose_context')?.loseContext()
  canvas.remove()
  const mean = (values) => values.reduce((sum, value) => sum + value, 0) / values.length
  return {
    snapshot,
    draw_ms: mean(drawMs),
    sync_ms: mean(syncMs),
    idle_sync_ms: mean(idleSyncMs),
    census: withCensus
      ? Object.fromEntries(
          [...census.entries()]
            .sort((a, b) => b[1].calls - a[1].calls)
            .map(([name, entry]) => [
              name,
              { calls: entry.calls / frames, ms: entry.ms / frames, bytes: entry.bytes / frames, largest: entry.largest },
            ]),
        )
      : null,
  }
}

/**
 * Raw WebGL2 with the map's shape of work and none of wgpu: `draws` indexed draws of Uint32 triangles
 * totalling `indices`, tiny triangles so fill costs nothing. `layout: 'map'` feeds the extrusion
 * pipeline's attributes (vec3 + vec2 + float per vertex, a mat4 + colour + vec2 per instance); `'plain'`
 * feeds one vec3. Returns the mean ms of draw + 1 px readback.
 */
export async function raw({ indices, draws, layout, usage = 'STATIC_DRAW', frames = 60 }) {
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = 1000
  document.body.append(canvas)
  const gl = canvas.getContext('webgl2', { antialias: false })
  const map = layout === 'map'
  const vertexCount = Math.ceil(indices * 0.6)
  const stride = map ? 24 : 12
  const vertexData = new Float32Array((vertexCount * stride) / 4)
  let center = [0, 0]
  for (let vertex = 0; vertex < vertexCount; vertex++) {
    const base = (vertex * stride) / 4
    // Neighbouring vertices sit within a few pixels of each other, like a building's facets.
    if (vertex % 16 === 0) center = [Math.random() * 2 - 1, Math.random() * 2 - 1]
    vertexData[base] = center[0] + Math.random() * 0.006
    vertexData[base + 1] = center[1] + Math.random() * 0.006
    vertexData[base + 2] = 0
    if (map) vertexData.set([0.6, 0.8, 1], base + 3)
  }
  const indexData = new Uint32Array(indices)
  for (let index = 0; index < indices; index++) indexData[index] = (index * 0.6 + (index % 3) * 0.0001) | 0
  const vertexBuffer = gl.createBuffer()
  // `usage` as wgpu's GL backend creates buffers: sized first, then filled.
  gl.bindBuffer(gl.ARRAY_BUFFER, vertexBuffer)
  gl.bufferData(gl.ARRAY_BUFFER, vertexData.byteLength, gl[usage])
  gl.bufferSubData(gl.ARRAY_BUFFER, 0, vertexData)
  const indexBuffer = gl.createBuffer()
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, indexBuffer)
  gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, indexData.byteLength, gl[usage])
  gl.bufferSubData(gl.ELEMENT_ARRAY_BUFFER, 0, indexData)
  const instanceBuffer = gl.createBuffer()
  gl.bindBuffer(gl.ARRAY_BUFFER, instanceBuffer)
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0.5, 0.5, 0.5, 1, 0.3, -0.5]), gl.STATIC_DRAW)
  const shader = (type, source) => {
    const object = gl.createShader(type)
    gl.shaderSource(object, source)
    gl.compileShader(object)
    return object
  }
  const vertexSource = map
    ? `#version 300 es
layout(location=0) in vec3 position; layout(location=1) in vec2 normal; layout(location=2) in float gradient;
layout(location=4) in vec4 t1; layout(location=5) in vec4 t2; layout(location=6) in vec4 t3; layout(location=7) in vec4 t4;
layout(location=8) in vec4 color; layout(location=14) in vec2 light_xy; out vec4 v_color;
void main() { vec3 n = vec3(normal, 1.0 - length(normal)); float light = clamp(dot(n, vec3(light_xy, 0.996)), 0.0, 1.0);
  v_color = vec4(color.rgb * light * gradient, 1.0); gl_Position = mat4(t1, t2, t3, t4) * vec4(position, 1.0); }`
    : `#version 300 es
layout(location=0) in vec3 position; out vec4 v_color; void main() { v_color = vec4(1.0); gl_Position = vec4(position, 1.0); }`
  const program = gl.createProgram()
  gl.attachShader(program, shader(gl.VERTEX_SHADER, vertexSource))
  gl.attachShader(
    program,
    shader(gl.FRAGMENT_SHADER, '#version 300 es\nprecision highp float; in vec4 v_color; out vec4 color; void main() { color = v_color; }'),
  )
  gl.linkProgram(program)
  gl.useProgram(program)
  gl.enable(gl.DEPTH_TEST)
  const pixel = new Uint8Array(4)
  const perDraw = Math.floor(indices / draws / 3) * 3
  const times = []
  for (let frame = 0; frame < frames + 10; frame++) {
    const started = performance.now()
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT)
    for (let draw = 0; draw < draws; draw++) {
      // Each draw re-points its attributes at its own range of the pool, as wgpu's GL backend does.
      gl.bindBuffer(gl.ARRAY_BUFFER, vertexBuffer)
      gl.enableVertexAttribArray(0)
      gl.vertexAttribPointer(0, 3, gl.FLOAT, false, stride, 0)
      if (map) {
        gl.enableVertexAttribArray(1)
        gl.vertexAttribPointer(1, 2, gl.FLOAT, false, stride, 12)
        gl.enableVertexAttribArray(2)
        gl.vertexAttribPointer(2, 1, gl.FLOAT, false, stride, 20)
        gl.bindBuffer(gl.ARRAY_BUFFER, instanceBuffer)
        for (const [location, size, offset] of [
          [4, 4, 0],
          [5, 4, 16],
          [6, 4, 32],
          [7, 4, 48],
          [8, 4, 64],
          [14, 2, 80],
        ]) {
          gl.enableVertexAttribArray(location)
          gl.vertexAttribPointer(location, size, gl.FLOAT, false, 88, offset)
          gl.vertexAttribDivisor(location, 1)
        }
      }
      gl.drawElementsInstanced(gl.TRIANGLES, perDraw, gl.UNSIGNED_INT, draw * perDraw * 4, 1)
    }
    gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel)
    if (frame >= 10) times.push(performance.now() - started)
  }
  gl.getExtension('WEBGL_lose_context')?.loseContext()
  canvas.remove()
  return times.reduce((sum, time) => sum + time, 0) / times.length
}
