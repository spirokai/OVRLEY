/** Browser-only instrumentation. Production modules and their scheduling stay unchanged. */

/** @returns {object} Timers, counters, and reversible browser/WASM wrappers. */
export function createInteractionProfiler() {
  let capture = null
  const restore = []
  const moveables = new WeakSet()
  const nativeFrame = window.requestAnimationFrame.bind(window)

  function record(name, duration) {
    if (capture === null) return
    const samples = (capture.stages[name] ??= [])
    samples.push(duration)
  }

  function count(name, amount = 1) {
    if (capture !== null) capture.counters[name] = (capture.counters[name] ?? 0) + amount
  }

  function measure(name, callback) {
    if (capture === null) return callback()
    const started = performance.now()
    try {
      return callback()
    } finally {
      record(name, performance.now() - started)
    }
  }

  function wrap(owner, key, label, bytes) {
    const original = owner[key]
    owner[key] = function (...args) {
      if (capture !== null && bytes) count(`${label}.bytes`, bytes(args))
      return measure(label, () => original.apply(this, args))
    }
    restore.push(() => (owner[key] = original))
  }

  return {
    record,
    count,
    measure,
    nextFrame: () => new Promise(nativeFrame),
    install(module) {
      wrap(module, '_ovrley_preview_widget_update', 'wasm.config_update', (args) => args[3])
      wrap(module, '_ovrley_preview_resize', 'wasm.resize', (args) => args[2])
      wrap(module, '_ovrley_preview_render', 'wasm.render')
      wrap(JSON, 'stringify', 'json.stringify')
      wrap(JSON, 'parse', 'json.parse')
      wrap(CanvasRenderingContext2D.prototype, 'putImageData', 'canvas.upload', (args) => args[0].data.byteLength)
      wrap(Uint8ClampedArray.prototype, 'set', 'pixels.copy', (args) => args[0].byteLength)
      wrap(Element.prototype, 'getBoundingClientRect', 'dom.measure')
      window.requestAnimationFrame = (callback) => nativeFrame((time) => measure(`raf.${callback.name || 'anonymous'}`, () => callback(time)))
      restore.push(() => (window.requestAnimationFrame = nativeFrame))
    },
    watchMoveable(moveable) {
      if (moveable === null || moveables.has(moveable)) return
      moveables.add(moveable)
      wrap(moveable, 'updateRect', 'moveable.updateRect')
    },
    start() {
      if (capture !== null) throw new Error('An interaction profile is already running')
      capture = { stages: {}, counters: { 'geometry.presentations': 0, 'canvas.upload.bytes': 0 } }
    },
    stop() {
      if (capture === null) throw new Error('No interaction profile is running')
      const result = capture
      capture = null
      return result
    },
    dispose() {
      capture = null
      for (const undo of restore.reverse()) undo()
    },
  }
}
