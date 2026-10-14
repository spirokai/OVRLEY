import { previewActivity, previewFont, rasterPreviewPng } from '@/api/backend'
import { isTimeDependentWasmWidget } from '@/lib/widget/wasm-widget'
import { createWidgetInteraction, registerWidgetInteraction, unregisterWidgetInteraction } from './widgetInteraction'
import { presentWidgetGeometry } from './widgetPresentation'

const BASE_URL = '/wasm-bindings'
const encoder = new TextEncoder()
const decoder = new TextDecoder()
let modulePromise

function widgetFonts(widget) {
  if (widget.type === 'backdrop' || widget.type === 'raster') return []
  if (['linear', 'arc', 'corner'].includes(widget.data.display_type)) {
    const fonts = widget.data.display_type === 'linear' ? [] : [{ font: widget.data.font }]
    if (widget.data.show_min_max_labels) fonts.push({ font: widget.data.min_max_label_font })
    return fonts
  }
  const valueFont = {
    font: widget.type === 'g_force' || widget.data.display_type === 'heading_tape' ? widget.data.label_font : widget.data.font,
    ...(widget.type === 'label' ? { font_weight: widget.data.font_weight, italic: widget.data.italic } : {}),
  }
  return widget.type === 'lap_timer' ? [valueFont, { font: widget.data.label_font }] : [valueFont]
}

/** @returns {Promise<object>} One compiled module shared by independently disposable editor sessions. */
export async function loadWidgetModule() {
  modulePromise ??= import(/* @vite-ignore */ `${BASE_URL}/wasm_bindings.js`).then(({ default: createModule }) =>
    createModule({ locateFile: (name) => `${BASE_URL}/${name}` }),
  )
  return modulePromise
}

/**
 * External ABI boundary. Rust owns sampling, layout, static caches and RGBA surfaces.
 * JS owns scheduling, resource transport and presentation. No activity JSON is parsed here.
 * @param {object} module Emscripten module.
 * @param {Function} onError Visible error reporting.
 * @returns {object} Disposable editor preview session.
 */
export function createPreviewSession(module, onError) {
  const handle = module._ovrley_preview_create()
  const widgets = new Map()
  const canvases = new Map()
  const fonts = new Map()
  const images = new Map()
  let snapshot
  let activityId
  let activityReady = true
  let disposed = false
  let frame = null
  let renderError = null
  const interaction = createWidgetInteraction({
    schedule,
    flush() {
      if (frame !== null) cancelAnimationFrame(frame)
      render()
      if (renderError) throw renderError
    },
    getSnapshot: () => snapshot,
    getGeometry: (id) => widgets.get(id).geometry,
    resize(request) {
      upload(encoder.encode(JSON.stringify(request)), (ptr, len) => check(module._ovrley_preview_resize(handle, ptr, len)))
      return JSON.parse(read(module._ovrley_preview_resize_ptr(handle), module._ovrley_preview_resize_len(handle)))
    },
  })

  function read(ptr, len) {
    return decoder.decode(module.HEAPU8.subarray(ptr, ptr + len))
  }
  function fail() {
    throw new Error(read(module._ovrley_preview_error_ptr(handle), module._ovrley_preview_error_len(handle)))
  }
  function check(status) {
    if (status !== 0) fail()
  }
  function upload(bytes, call) {
    const ptr = module._ovrley_alloc(bytes.byteLength)
    try {
      module.HEAPU8.set(bytes, ptr)
      return call(ptr, bytes.byteLength)
    } finally {
      module._ovrley_dealloc(ptr, bytes.byteLength)
    }
  }
  function schedule() {
    if (!disposed && frame === null) frame = requestAnimationFrame(render)
  }
  function loadFont(request) {
    const id = JSON.stringify(request)
    if (fonts.has(id)) return
    const resource = { ready: false, error: null }
    fonts.set(id, resource)
    previewFont(request)
      .then((bytes) => {
        if (disposed) return
        upload(encoder.encode(id), (idPtr, idLen) =>
          upload(new Uint8Array(bytes), (ptr, len) => check(module._ovrley_preview_font(handle, idPtr, idLen, ptr, len))),
        )
        resource.ready = true
        schedule()
      })
      .catch((error) => {
        if (disposed) return
        resource.error = error
        schedule()
      })
  }
  function loadImage(id) {
    if (images.has(id)) return
    const resource = { ready: false, error: null }
    images.set(id, resource)
    rasterPreviewPng(id)
      .then((base64) => {
        if (disposed) return
        const bytes = Uint8Array.from(atob(base64), (character) => character.charCodeAt(0))
        upload(encoder.encode(id), (idPtr, idLen) => upload(bytes, (ptr, len) => check(module._ovrley_preview_image(handle, idPtr, idLen, ptr, len))))
        resource.ready = true
        schedule()
      })
      .catch((error) => {
        resource.error = error
        schedule()
      })
  }
  function render() {
    frame = null
    if (!snapshot) return
    try {
      const defaultFont = fonts.get(JSON.stringify({ font: snapshot.defaultFont }))
      if (defaultFont.error) throw defaultFont.error
      const active = new Set(snapshot.widgets.map((widget) => widget.id))
      for (const [id, widget] of widgets) {
        if (!active.has(id)) {
          module._ovrley_preview_widget_destroy(handle, widget.handle)
          widgets.delete(id)
        }
      }
      for (const widget of interaction.resolve(snapshot)) {
        const resource = widget.type === 'raster' && images.get(widget.data.resourceId)
        if (resource?.error) throw resource.error
        if (resource && !resource.ready) {
          const instance = widgets.get(widget.id)
          if (instance) {
            module._ovrley_preview_widget_destroy(handle, instance.handle)
            widgets.delete(widget.id)
          }
          const canvas = canvases.get(widget.id)
          if (canvas) {
            canvas.getContext('2d').clearRect(0, 0, canvas.width, canvas.height)
            canvas.parentElement.style.visibility = 'hidden'
          }
          continue
        }
        const fontResources = widgetFonts(widget).map((font) => fonts.get(JSON.stringify(font)))
        for (const resource of fontResources) if (resource.error) throw resource.error
        if (fontResources.some((resource) => !resource.ready)) continue
        if (isTimeDependentWasmWidget(widget) && !activityReady) continue
        let instance = widgets.get(widget.id)
        if (instance?.data === widget.data && instance.scene === snapshot.scene) continue
        const config = JSON.stringify({ type: widget.type, widget: widget.data, scene: snapshot.scene })
        if (!instance) {
          const widgetHandle = upload(encoder.encode(config), (ptr, len) => module._ovrley_preview_widget_create(handle, ptr, len))
          if (!widgetHandle) fail()
          instance = { type: widget.type, handle: widgetHandle, config, revision: -1, image: null }
          widgets.set(widget.id, instance)
        } else if (config !== instance.config) {
          upload(encoder.encode(config), (ptr, len) => check(module._ovrley_preview_widget_update(handle, instance.handle, ptr, len)))
          instance.config = config
        }
        instance.data = widget.data
        instance.scene = snapshot.scene
      }
      if (activityReady && snapshot.widgets.some(isTimeDependentWasmWidget)) check(module._ovrley_preview_render(handle, snapshot.second))
      for (const [id, widget] of widgets) {
        if (isTimeDependentWasmWidget(widget) && !activityReady) continue
        const canvas = canvases.get(id)
        if (!canvas) continue
        const revision = module._ovrley_preview_revision(handle, widget.handle)
        if (widget.revision === revision) continue
        const geometry = JSON.parse(
          read(module._ovrley_preview_geometry_ptr(handle, widget.handle), module._ovrley_preview_geometry_len(handle, widget.handle)),
        )
        if (!widget.image || widget.image.width !== geometry.width || widget.image.height !== geometry.height) {
          canvas.width = geometry.width
          canvas.height = geometry.height
          widget.image = new ImageData(geometry.width, geometry.height)
        }
        const ptr = module._ovrley_preview_pixels(handle, widget.handle)
        widget.image.data.set(module.HEAPU8.subarray(ptr, ptr + widget.image.data.length))
        canvas.getContext('2d').putImageData(widget.image, 0, 0)
        presentWidgetGeometry(canvas, geometry)
        widget.geometry = geometry
        widget.revision = revision
      }
      renderError = null
      onError(null)
    } catch (error) {
      renderError = error
      onError(error)
    }
  }
  return {
    setSnapshot(next) {
      if (next.activityId === undefined) {
        onError(new Error('Finalized activity is missing its native preview identity. Reload the activity.'))
        return
      }
      const previous = snapshot
      snapshot = next
      // A playhead-only update has no work to schedule for a static scene.
      if (
        previous &&
        previous.widgets === next.widgets &&
        previous.scene === next.scene &&
        previous.activityId === next.activityId &&
        previous.defaultFont === next.defaultFont &&
        !next.widgets.some(isTimeDependentWasmWidget)
      )
        return
      loadFont({ font: next.defaultFont })
      for (const widget of next.widgets) {
        for (const font of widgetFonts(widget)) loadFont(font)
        if (widget.type === 'raster' && widget.data.resourceId !== undefined) loadImage(widget.data.resourceId)
      }
      const nextId = next.activityId
      if (nextId !== activityId) {
        activityId = nextId
        activityReady = false
        // Explicitly clear old values while replacement bytes are in flight.
        check(module._ovrley_preview_activity(handle, 0, 0))
        for (const [id, widget] of widgets) {
          if (!isTimeDependentWasmWidget(widget)) continue
          const canvas = canvases.get(id)
          if (canvas) canvas.getContext('2d').clearRect(0, 0, canvas.width, canvas.height)
          widget.revision = -1
        }
        if (nextId === null) activityReady = true
        else
          previewActivity(nextId)
            .then((bytes) => {
              if (disposed || activityId !== nextId) return
              upload(new Uint8Array(bytes), (ptr, len) => check(module._ovrley_preview_activity(handle, ptr, len)))
              activityReady = true
              for (const widget of widgets.values()) if (isTimeDependentWasmWidget(widget)) widget.revision = -1
              schedule()
            })
            .catch((error) => {
              if (!disposed && activityId === nextId) onError(error)
            })
      }
      schedule()
    },
    attach(id, canvas) {
      const previous = canvases.get(id)
      if (previous) unregisterWidgetInteraction(previous.parentElement)
      if (canvas) {
        canvases.set(id, canvas)
        registerWidgetInteraction(canvas.parentElement, interaction.forWidget(id))
      } else canvases.delete(id)
      const widget = widgets.get(id)
      if (widget) {
        widget.revision = -1
        widget.image = null
      }
      schedule()
    },
    dispose() {
      disposed = true
      if (frame !== null) cancelAnimationFrame(frame)
      interaction.dispose()
      for (const canvas of canvases.values()) unregisterWidgetInteraction(canvas.parentElement)
      module._ovrley_preview_destroy(handle)
      widgets.clear()
      canvases.clear()
      fonts.clear()
      images.clear()
    },
  }
}
