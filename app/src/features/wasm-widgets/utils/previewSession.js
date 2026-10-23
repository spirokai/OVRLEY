import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { previewActivity, previewFont, rasterPreviewPng } from '@/api/backend'
import { isPreviewPerfEnabled, recordPreviewPerfDuration, setPreviewPerfValue } from '@/lib/preview-perf'
import { createWidgetInteraction, registerWidgetInteraction, unregisterWidgetInteraction } from './widgetInteraction'
import { presentWidgetGeometry } from './widgetPresentation'

const BASE_URL = '/wasm-bindings'
const encoder = new TextEncoder()
const decoder = new TextDecoder()
let modulePromise

function isTimeDependentWidget(widget) {
  return WIDGET_TYPE_DEFINITIONS[widget.type].timeDependent
}

/** @param {object} widget Effective widget. @returns {object[]} Required native font resource requests. */
export function widgetFonts(widget) {
  if (widget.type === 'backdrop' || widget.type === 'raster' || widget.type === 'course') return []
  if (widget.type === 'diving_profile') {
    return widget.data.show_depth_metric || widget.data.show_depth_imperial ? [{ font: widget.data.point_label.font }] : []
  }
  if (widget.type === 'elevation') {
    return widget.data.show_elevation_metric || widget.data.show_elevation_imperial ? [{ font: widget.data.point_label.font }] : []
  }
  if (['linear', 'arc', 'corner', 'gt_gauge'].includes(widget.data.display_type)) {
    const fonts = widget.data.display_type === 'linear' ? [] : [{ font: widget.data.font }]
    if (widget.data.show_min_max_labels) fonts.push({ font: widget.data.min_max_label_font })
    return fonts
  }
  if (widget.data.display_type === 'heart_zones') {
    return widget.data.show_label ? [{ font: widget.data.font, font_weight: widget.data.label_font_weight }] : []
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
    createModule({ locateFile: (name) => `${BASE_URL}/${name}` }).then(guardModule),
  )
  return modulePromise
}

/**
 * A trap leaves Rust state half-mutated, so every later call would fail with unrelated memory errors.
 * The first trap is converted into one terminal error carrying the recorded Rust panic.
 * @param {object} module Emscripten module.
 * @returns {object} Module whose `_ovrley_*` exports throw the terminal crash once trapped.
 */
function guardModule(module) {
  const guarded = Object.create(module)
  guarded.crash = null
  for (const name of Object.keys(module)) {
    if (!name.startsWith('_ovrley_')) continue
    const exported = module[name]
    guarded[name] = (...args) => {
      if (guarded.crash) throw guarded.crash
      try {
        return exported(...args)
      } catch (error) {
        if (!(error instanceof WebAssembly.RuntimeError)) throw error
        const panicPtr = module._ovrley_panic_ptr()
        const panic = decoder.decode(module.HEAPU8.subarray(panicPtr, panicPtr + module._ovrley_panic_len()))
        const cause = panic || `${error.message} (no Rust panic was recorded)`
        guarded.crash = new Error(`WASM renderer crashed: ${cause}. Restart OVRLEY.`, { cause: error })
        // The next editor mount instantiates a fresh module.
        modulePromise = undefined
        throw guarded.crash
      }
    }
  }
  return guarded
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
  const targets = new Map()
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
    resize(request, id) {
      const input = ['course', 'elevation', 'diving_profile'].includes(request.type) ? { ...request, handle: widgets.get(id).handle } : request
      upload(encoder.encode(JSON.stringify(input)), (ptr, len) => check(module._ovrley_preview_resize(handle, ptr, len)))
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
    const measure = isPreviewPerfEnabled()
    const frameStarted = measure ? performance.now() : 0
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
        let instance = widgets.get(widget.id)
        const transient =
          interaction.isDragging(widget.id) || (['course', 'elevation', 'diving_profile'].includes(widget.type) && interaction.isResizing(widget.id))
        if (instance?.data === widget.data && instance.scene === snapshot.scene && instance.transient === transient) continue
        const resource = widget.type === 'raster' && images.get(widget.data.resourceId)
        if (resource?.error) throw resource.error
        if (resource && !resource.ready) {
          if (instance) {
            module._ovrley_preview_widget_destroy(handle, instance.handle)
            widgets.delete(widget.id)
          }
          const canvas = canvases.get(widget.id)
          if (canvas) {
            canvas.getContext('2d').clearRect(0, 0, canvas.width, canvas.height)
            targets.get(widget.id).style.visibility = 'hidden'
          }
          continue
        }
        const fontResources = widgetFonts(widget).map((font) => fonts.get(JSON.stringify(font)))
        for (const resource of fontResources) if (resource.error) throw resource.error
        if (fontResources.some((resource) => !resource.ready)) continue
        if (WIDGET_TYPE_DEFINITIONS[widget.type].timeDependent && !activityReady) continue
        const config = JSON.stringify({
          type: widget.type,
          widget: widget.data,
          scene: snapshot.scene,
          transient,
        })
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
        instance.transient = transient
      }
      if (activityReady && snapshot.widgets.some(isTimeDependentWidget)) {
        const started = measure ? performance.now() : 0
        check(module._ovrley_preview_render(handle, snapshot.second))
        if (measure) recordPreviewPerfDuration('WASM render (all widgets)', performance.now() - started)
      }
      for (const [id, widget] of widgets) {
        if (WIDGET_TYPE_DEFINITIONS[widget.type].timeDependent && !activityReady) continue
        const canvas = canvases.get(id)
        if (!canvas) continue
        const revision = module._ovrley_preview_revision(handle, widget.handle)
        if (widget.revision === revision) continue
        const geometryJson = read(
          module._ovrley_preview_geometry_ptr(handle, widget.handle),
          module._ovrley_preview_geometry_len(handle, widget.handle),
        )
        const geometryChanged = widget.geometryJson !== geometryJson
        const geometry = geometryChanged ? JSON.parse(geometryJson) : widget.geometry
        const pixelsRevision = module._ovrley_preview_pixels_revision(handle, widget.handle)
        if (!widget.image || widget.pixelsRevision !== pixelsRevision) {
          if (!widget.image || widget.image.width !== geometry.width || widget.image.height !== geometry.height) {
            canvas.width = geometry.width
            canvas.height = geometry.height
            widget.image = new ImageData(geometry.width, geometry.height)
          }
          const measurePixels =
            measure && (widget.type === 'course' || widget.type === 'elevation' || widget.type === 'diving_profile' || widget.type === 'raster')
          const widgetLabel = measurePixels ? `${widget.type}:${id}` : null
          const copyStarted = measurePixels ? performance.now() : 0
          const ptr = module._ovrley_preview_pixels(handle, widget.handle)
          widget.image.data.set(module.HEAPU8.subarray(ptr, ptr + widget.image.data.length))
          const uploadStarted = measurePixels ? performance.now() : 0
          canvas.getContext('2d').putImageData(widget.image, 0, 0)
          if (measurePixels) {
            const uploaded = performance.now()
            recordPreviewPerfDuration(`${widgetLabel} pixel copy`, uploadStarted - copyStarted)
            recordPreviewPerfDuration(`${widgetLabel} canvas upload`, uploaded - uploadStarted)
            setPreviewPerfValue(`${widgetLabel} raster`, `${geometry.width} x ${geometry.height}`)
          }
          widget.pixelsRevision = pixelsRevision
        }
        // Pixel-only playback changes do not require DOM writes or a Moveable measurement.
        if (geometryChanged) presentWidgetGeometry(canvas, geometry, targets.get(id), widget.type)
        widget.geometry = geometry
        widget.geometryJson = geometryJson
        widget.revision = revision
      }
      renderError = null
      onError(null)
      if (measure) recordPreviewPerfDuration('WASM session frame', performance.now() - frameStarted)
    } catch (error) {
      renderError = error
      onError(error)
    }
  }
  return {
    setSnapshot(next) {
      if (module.crash) {
        onError(module.crash)
        return
      }
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
        previous.disabledLaps === next.disabledLaps &&
        previous.defaultFont === next.defaultFont &&
        !next.widgets.some(isTimeDependentWidget)
      )
        return
      if (previous?.defaultFont !== next.defaultFont) loadFont({ font: next.defaultFont })
      if (previous?.widgets !== next.widgets) {
        for (const widget of next.widgets) {
          for (const font of widgetFonts(widget)) loadFont(font)
          if (widget.type === 'raster' && widget.data.resourceId !== undefined) loadImage(widget.data.resourceId)
        }
      }
      const nextId = next.activityId
      if (nextId !== activityId) {
        activityId = nextId
        activityReady = false
        // Explicitly clear old values while replacement bytes are in flight.
        check(module._ovrley_preview_activity(handle, 0, 0))
        for (const [id, widget] of widgets) {
          if (!WIDGET_TYPE_DEFINITIONS[widget.type].timeDependent) continue
          const canvas = canvases.get(id)
          if (canvas) canvas.getContext('2d').clearRect(0, 0, canvas.width, canvas.height)
          widget.revision = -1
          widget.pixelsRevision = -1
        }
        if (nextId === null) activityReady = true
        else
          previewActivity(nextId)
            .then((bytes) => {
              if (disposed || activityId !== nextId) return
              upload(new Uint8Array(bytes), (ptr, len) => check(module._ovrley_preview_activity(handle, ptr, len)))
              activityReady = true
              for (const widget of widgets.values()) if (WIDGET_TYPE_DEFINITIONS[widget.type].timeDependent) widget.revision = -1
              schedule()
            })
            .catch((error) => {
              if (!disposed && activityId === nextId) onError(error)
            })
      }
      // The session applies the selection to in-flight activity bytes on arrival.
      if (previous?.disabledLaps !== next.disabledLaps) {
        try {
          upload(encoder.encode(JSON.stringify(next.disabledLaps)), (ptr, len) => check(module._ovrley_preview_disabled_laps(handle, ptr, len)))
        } catch (error) {
          onError(error)
          return
        }
        for (const widget of widgets.values()) if (WIDGET_TYPE_DEFINITIONS[widget.type].timeDependent) widget.revision = -1
      }
      schedule()
    },
    attach(id, canvas, target = canvas?.parentElement) {
      const previous = targets.get(id)
      if (previous) unregisterWidgetInteraction(previous)
      if (canvas) {
        canvases.set(id, canvas)
        targets.set(id, target)
        registerWidgetInteraction(target, interaction.forWidget(id))
      } else {
        canvases.delete(id)
        targets.delete(id)
      }
      const widget = widgets.get(id)
      if (widget) {
        widget.revision = -1
        widget.image = null
        widget.geometryJson = null
      }
      schedule()
    },
    dispose() {
      disposed = true
      if (frame !== null) cancelAnimationFrame(frame)
      interaction.dispose()
      // Wrappers stay hidden until a session presents them again, so nothing reads their geometry.
      for (const target of targets.values()) {
        unregisterWidgetInteraction(target)
        target.style.visibility = 'hidden'
      }
      module._ovrley_preview_destroy(handle)
      widgets.clear()
      canvases.clear()
      targets.clear()
      fonts.clear()
      images.clear()
    },
  }
}
