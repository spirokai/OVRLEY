import { getMapStyleUrlTemplate } from '@/api/backend'
import { createMapTileWorkers } from './mapTileWorkers'

const BASE_URL = '/wasm-bindings'
const VECTOR_SOURCE = 'openmaptiles'
/** A run of zoom edits draws live on the current renderer, and only what it settles on is rebuilt. */
const STYLE_ZOOM_SETTLE_MS = 300
/** The most a replacement waits for its tiles. It then shows what it has, so a slow network can't hide an edit. */
const REPLACE_TIMEOUT_MS = 1500
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
 * @param {string} name Installed map style id.
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
 *
 * Style zoom is fixed for a renderer's life, because tiles load against it, but its camera isn't. So
 * a zoom edit draws straight away on the current renderer, and a rebuild only follows once it settles.
 * Its size isn't fixed either, so a resize draws at the new size with every tile kept.
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
      state = {
        holder: null,
        wanted: null,
        wantedIdentity: null,
        current: null,
        replacement: null,
        drawn: null,
        failedKey: null,
        settle: null,
        settling: null,
        replaceTimer: null,
      }
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

  function clearReplacement(state) {
    clearTimeout(state.replaceTimer)
    if (state.replacement) release(state.replacement)
    state.replacement = null
  }

  function clearRenderers(state) {
    clearTimeout(state.settle)
    state.settling = null
    state.wanted = null
    state.wantedIdentity = null
    clearReplacement(state)
    if (state.current) release(state.current)
    state.current = null
    state.drawn = null
  }

  function fail(id, state, key, error) {
    console.warn(`Map preview unavailable: ${error?.message ?? error}`)
    state.failedKey = key
    clearRenderers(state)
    if (state.holder) state.holder.style.visibility = 'hidden'
    setReady(id, false)
    schedule()
  }

  /** @param {object} renderer A built renderer, which now shows in the widget. */
  function show(id, state, renderer) {
    clearTimeout(state.replaceTimer)
    state.replacement = null
    if (state.current) release(state.current)
    state.current = renderer
    state.drawn = null
    state.holder.replaceChildren(renderer.canvas)
    setReady(id, true)
    schedule()
  }

  /** Builds a renderer on a new canvas. It replaces a current one only once it has its tiles, so an edit never flashes an empty map. */
  async function build(id, state, view, key) {
    const { renderer, width, height, style_zoom: zoom } = view
    state.wanted = key
    state.wantedIdentity = JSON.stringify(renderer)
    const config = JSON.stringify({ ...renderer, width, height, zoom })
    try {
      const [module, style] = await Promise.all([loadMapModule(), loadStyle(renderer.style)])
      const canvas = document.createElement('canvas')
      canvas.width = width
      canvas.height = height
      canvas.style.cssText = 'display:block;width:100%;height:100%'
      const layer = await module.MapLayer.create(canvas, config, style.bytes)
      if (disposed || state.wanted !== key || !state.holder) {
        layer.free()
        return
      }
      const built = {
        key,
        width,
        height,
        pixelRatio: renderer.pixel_ratio,
        canvas,
        layer,
        routeId: null,
        workers: createMapTileWorkers({
          config,
          style: style.bytes,
          tileUrl: style.tileUrl,
          onTile(z, x, y, tile) {
            if (state.current !== built && state.replacement !== built) return
            try {
              layer.upload(z, x, y, tile)
            } catch (error) {
              fail(id, state, key, error)
              return
            }
            state.drawn = null
            schedule()
          },
          onFailed() {
            state.drawn = null
            schedule()
          },
          onFatal: (error) => fail(id, state, key, error),
        }),
      }
      // Releasing a replaced renderer loses its context on purpose, so only a live one counts.
      canvas.addEventListener('webglcontextlost', () => {
        if (state.current === built || state.replacement === built) fail(id, state, key, new Error('WebGL2 context lost'))
      })
      if (!state.current) {
        show(id, state, built)
        return
      }
      clearReplacement(state)
      state.replacement = built
      state.replaceTimer = setTimeout(() => {
        if (state.replacement === built && state.wanted === key) show(id, state, built)
      }, REPLACE_TIMEOUT_MS)
      state.drawn = null
      schedule()
    } catch (error) {
      if (!disposed && state.wanted === key) fail(id, state, key, error)
    }
  }

  /** @param {object} renderer A renderer to draw at the view's size, which keeps its tiles. @param {object} view The widget's view. */
  function fitSize(renderer, view) {
    if (renderer.width === view.width && renderer.height === view.height) return
    renderer.layer.resize(view.width, view.height)
    renderer.width = view.width
    renderer.height = view.height
  }

  /**
   * Starts whichever renderer this view needs: at once when its config changed, or after the settle
   * when only its style zoom did. Playback calls this every frame, so an unchanged wish keeps its timer.
   */
  function wantRenderer(id, state, view, key) {
    if (key === state.settling) return
    clearTimeout(state.settle)
    state.settling = null
    if (key === state.wanted) return
    const identity = JSON.stringify(view.renderer)
    // The edit came back to what is showing, so nothing on its way is wanted any more.
    if (key === state.current?.key) {
      state.wanted = key
      state.wantedIdentity = identity
      clearReplacement(state)
      return
    }
    if (identity !== state.wantedIdentity) {
      build(id, state, view, key)
      return
    }
    state.settling = key
    state.settle = setTimeout(() => {
      state.settling = null
      if (!disposed && state.holder) build(id, state, view, key)
    }, STYLE_ZOOM_SETTLE_MS)
  }

  return {
    /** @param {string} id Widget id. @param {HTMLElement|null} holder Element the map canvas lives in. */
    attach(id, holder) {
      const state = layerState(id)
      state.holder = holder
      if (!holder) {
        clearRenderers(state)
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
      // Size isn't part of a renderer's identity, since it can draw at any.
      const key = JSON.stringify({ ...view.renderer, zoom: view.style_zoom })
      if (key === state.failedKey) return
      wantRenderer(id, state, view, key)
      const camera = Float64Array.from(view.camera)
      const labels = Float64Array.from(view.labels)
      state.holder.style.opacity = `${view.opacity}`
      try {
        // A replacement fetches for the view it is about to show. Nothing left to wait for means it can show it.
        const replacement = state.replacement
        if (replacement?.key === key) {
          fitSize(replacement, view)
          if (replacement.workers.want(replacement.layer.missing(camera, labels)) === 0) show(id, state, replacement)
        }
        const renderer = state.current
        // A renderer of another pixel density keeps its last frame, rescaled, until its replacement can draw.
        if (renderer?.pixelRatio !== view.renderer.pixel_ratio) return
        fitSize(renderer, view)
        // A top-down view has no route: the widget strokes it over the canvas, which draws only the map.
        const { route } = view
        if (route && renderer.routeId !== route.id) {
          renderer.layer.set_route(readRoute())
          renderer.routeId = route.id
        }
        renderer.workers.want(renderer.layer.missing(camera, labels))
        if (route) renderer.layer.draw_with_route(camera, labels, Float32Array.from(route.style), route.travelled_metres, view.corner_radius_px)
        else renderer.layer.draw(camera, labels, view.corner_radius_px)
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
      state.failedKey = null
      clearRenderers(state)
    },

    dispose() {
      disposed = true
      for (const state of layers.values()) clearRenderers(state)
      layers.clear()
    },
  }
}
