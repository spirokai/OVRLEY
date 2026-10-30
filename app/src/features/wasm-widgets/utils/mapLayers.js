import { getMapStyleUrlTemplate } from '@/api/backend'
import { createMapTileWorkers } from './mapTileWorkers'

const BASE_URL = '/wasm-bindings'
const VECTOR_SOURCE = 'openmaptiles'
let modulePromise
const styles = new Map()

/** @returns {Promise<object>} The map module. It loads only when a project has a map. */
function loadMapModule() {
  modulePromise ??= import(/* @vite-ignore */ `${BASE_URL}/wasm_map_bindings.js`).then(async (module) => {
    await module.default({ module_or_path: `${BASE_URL}/wasm_map_bindings_bg.wasm` })
    return module
  })
  return modulePromise
}

/**
 * @param {string} name Map style preset.
 * @returns {Promise<{bytes: Uint8Array, tileUrl: string}>} Style document and the tile URL template it names.
 */
function loadStyle(name) {
  if (styles.has(name)) return styles.get(name)
  const loading = getMapStyleUrlTemplate()
    .then((template) => {
      if (template === null) throw new Error('The map service only runs in the desktop app')
      return fetch(template.replace('{style}', name))
    })
    .then((response) => {
      if (!response.ok) throw new Error(`Map style ${name}: HTTP ${response.status}`)
      return response.arrayBuffer()
    })
    .then((buffer) => {
      const bytes = new Uint8Array(buffer)
      return { bytes, tileUrl: JSON.parse(new TextDecoder().decode(bytes)).sources[VECTOR_SOURCE].tiles[0] }
    })
  loading.catch(() => styles.delete(name))
  styles.set(name, loading)
  return loading
}

/**
 * The map canvases under map widgets, drawn in the session's animation frame. WebGL2 is optional, so
 * every failure here ends at the map: the widget keeps its placeholder and nothing reaches the session.
 * @param {object} host Session callbacks.
 * @param {Function} host.schedule Requests a session frame.
 * @param {Function} host.setReady Tells a widget whether its canvas is showing.
 * @returns {object} Map layers of one preview session.
 */
export function createMapLayers({ schedule, setReady }) {
  const layers = new Map()
  let disposed = false

  function layerState(id) {
    let state = layers.get(id)
    if (!state) {
      state = { holder: null, wanted: null, current: null, drawn: null, failedKey: null }
      layers.set(id, state)
    }
    return state
  }

  function release(renderer) {
    renderer.workers.dispose()
    renderer.layer.free()
    renderer.canvas.getContext('webgl2')?.getExtension('WEBGL_lose_context')?.loseContext()
    renderer.canvas.remove()
  }

  function fail(id, state, key, error) {
    console.warn(`Map preview unavailable: ${error?.message ?? error}`)
    state.failedKey = key
    state.wanted = null
    if (state.current) release(state.current)
    state.current = null
    state.drawn = null
    if (state.holder) state.holder.style.visibility = 'hidden'
    setReady(id, false)
    schedule()
  }

  /** Builds a renderer on a new canvas, which replaces the old one only once it can draw. */
  async function build(id, state, key, config) {
    state.wanted = key
    try {
      const [module, style] = await Promise.all([loadMapModule(), loadStyle(config.style)])
      const canvas = document.createElement('canvas')
      canvas.width = config.width
      canvas.height = config.height
      canvas.style.cssText = 'display:block;width:100%;height:100%'
      const layer = await module.MapLayer.create(canvas, key, style.bytes)
      if (disposed || state.wanted !== key || !state.holder) {
        layer.free()
        return
      }
      const renderer = {
        key,
        canvas,
        layer,
        routeId: null,
        workers: createMapTileWorkers({
          config: key,
          style: style.bytes,
          tileUrl: style.tileUrl,
          onTile(z, x, y, tile) {
            if (state.current !== renderer) return
            try {
              layer.upload(z, x, y, tile)
            } catch (error) {
              fail(id, state, key, error)
              return
            }
            state.drawn = null
            schedule()
          },
          onFatal: (error) => fail(id, state, key, error),
        }),
      }
      // Releasing a replaced renderer loses its context on purpose, so only the current one counts.
      canvas.addEventListener('webglcontextlost', () => {
        if (state.current === renderer) fail(id, state, key, new Error('WebGL2 context lost'))
      })
      if (state.current) release(state.current)
      state.current = renderer
      state.drawn = null
      state.holder.replaceChildren(canvas)
      setReady(id, true)
      schedule()
    } catch (error) {
      if (!disposed && state.wanted === key) fail(id, state, key, error)
    }
  }

  return {
    /** @param {string} id Widget id. @param {HTMLElement|null} holder Element the map canvas lives in. */
    attach(id, holder) {
      const state = layerState(id)
      state.holder = holder
      if (!holder) {
        state.wanted = null
        if (state.current) release(state.current)
        state.current = null
        state.drawn = null
        setReady(id, false)
      }
    },

    /**
     * Draws a widget's view. A view with no change and no new tiles draws nothing.
     * @param {string} id Widget id.
     * @param {string} json The widget's view, empty when the map draws nothing.
     * @param {Function} readRoute Reads the widget's route line.
     */
    present(id, json, readRoute) {
      const state = layers.get(id)
      if (!state?.holder) return
      if (!json) {
        state.holder.style.visibility = 'hidden'
        state.drawn = null
        return
      }
      if (json === state.drawn) return
      const view = JSON.parse(json)
      const key = JSON.stringify(view.renderer)
      if (key === state.failedKey) return
      if (key !== state.current?.key && key !== state.wanted) build(id, state, key, view.renderer)
      const renderer = state.current
      // A replaced renderer keeps showing, rescaled, until the new one can draw.
      if (renderer?.key !== key) return
      try {
        if (renderer.routeId !== view.route_id) {
          renderer.layer.set_route(readRoute())
          renderer.routeId = view.route_id
        }
        const camera = Float64Array.from(view.camera)
        renderer.workers.want(renderer.layer.missing(camera))
        renderer.layer.draw(camera, Float32Array.from(view.route_style), view.travelled_metres)
      } catch (error) {
        fail(id, state, key, error)
        return
      }
      state.holder.style.visibility = ''
      state.drawn = json
    },

    /** @param {string} id Widget id whose handle is gone. A later one starts over. */
    remove(id) {
      const state = layers.get(id)
      if (!state) return
      state.wanted = null
      state.failedKey = null
      if (state.current) release(state.current)
      state.current = null
      state.drawn = null
    },

    dispose() {
      disposed = true
      for (const state of layers.values()) if (state.current) release(state.current)
      layers.clear()
    },
  }
}
