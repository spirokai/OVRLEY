/**
 * Browser loader for the generated Emscripten Wasm preview POC.
 *
 * Performance note: all buffer allocations are hoisted out of the hot path.
 * The RGBA buffer (32MB) and string buffers are allocated once and reused
 * across frames to avoid per-frame alloc/dealloc churn.
 */

const ARTIFACT_BASE_URL = '/debug/wasm-preview-artifacts'
const SCRIPT_URL = `${ARTIFACT_BASE_URL}/wasm_preview_poc.js`
const WASM_URL = `${ARTIFACT_BASE_URL}/wasm_preview_poc.wasm`
const BACKEND_LABELS = {
  1: 'software-raster-rgba8888',
}

let modulePromise = null
let fontLoaded = false

// Persistent per-frame buffers — allocated once, reused every frame.
let persistentRgbaPtr = 0
let persistentRgbaLen = 0
let persistentValuePtr = 0
let persistentValueCap = 0
let persistentUnitPtr = 0
let persistentUnitCap = 0
const textEncoder = new TextEncoder()

// Persistent per-canvas resources — reused across frames so multi-surface
// benchmarks do not reallocate ImageData on every widget draw.
const canvasResourceCache = new WeakMap()

/**
 * Returns cached drawing resources for a canvas, recreating them only when the
 * backing size changes.
 *
 * @param {HTMLCanvasElement} canvas - Target canvas.
 * @param {number} width - Backing width in pixels.
 * @param {number} height - Backing height in pixels.
 * @returns {{ context: CanvasRenderingContext2D, imageData: ImageData }} Cached resources.
 */
function getCanvasResources(canvas, width, height) {
  const cachedResources = canvasResourceCache.get(canvas)
  if (cachedResources?.width === width && cachedResources?.height === height) {
    return cachedResources
  }

  canvas.width = width
  canvas.height = height

  const context = canvas.getContext('2d')
  if (!context) {
    throw new Error('Browser did not provide a 2D canvas context.')
  }

  const nextResources = {
    context,
    height,
    imageData: new ImageData(width, height),
    width,
  }

  canvasResourceCache.set(canvas, nextResources)
  return nextResources
}

/**
 * Converts an unknown thrown value into a message.
 *
 * @param {*} error - Thrown value.
 * @returns {string} User-facing error message.
 */
function describeError(error) {
  if (error instanceof Error) {
    return error.message
  }
  if (typeof error === 'string') {
    return error
  }
  return 'Unknown Wasm preview error'
}

/**
 * Ensures the persistent RGBA buffer is allocated to the required size.
 *
 * @param {object} module - Emscripten module.
 * @param {number} requiredLen - Required buffer length in bytes.
 */
function ensureRgbaBuffer(module, requiredLen) {
  if (persistentRgbaPtr && persistentRgbaLen >= requiredLen) {
    return
  }
  if (persistentRgbaPtr) {
    module._ovrley_wasm_preview_dealloc(persistentRgbaPtr, persistentRgbaLen)
  }
  persistentRgbaPtr = module._ovrley_wasm_preview_alloc(requiredLen)
  persistentRgbaLen = requiredLen
  if (!persistentRgbaPtr) {
    throw new Error(`RGBA buffer allocation failed for ${requiredLen} bytes.`)
  }
}

/**
 * Ensures a persistent string buffer is large enough for the given byte length.
 *
 * @param {object} module - Emscripten module.
 * @param {number} requiredLen - Required capacity in bytes.
 * @param {'value'|'unit'} which - Which buffer to ensure.
 * @returns {{ ptr: number, len: number }} Pointer and length.
 */
function ensureStringBuffer(module, requiredLen, which) {
  const isValue = which === 'value'
  let ptr = isValue ? persistentValuePtr : persistentUnitPtr
  let cap = isValue ? persistentValueCap : persistentUnitCap

  if (ptr && cap >= requiredLen) {
    return { ptr, len: requiredLen }
  }

  if (ptr) {
    module._ovrley_wasm_preview_dealloc(ptr, cap)
  }
  ptr = module._ovrley_wasm_preview_alloc(requiredLen)
  cap = requiredLen

  if (!ptr) {
    throw new Error(`${which} string allocation failed for ${requiredLen} bytes.`)
  }

  if (isValue) {
    persistentValuePtr = ptr
    persistentValueCap = cap
  } else {
    persistentUnitPtr = ptr
    persistentUnitCap = cap
  }

  return { ptr, len: requiredLen }
}

/**
 * Loads the generated Emscripten runtime once.
 *
 * @returns {Promise<object>} Initialized Emscripten module.
 */
export function loadWasmPreviewModule() {
  if (modulePromise) {
    return modulePromise
  }

  modulePromise = new Promise((resolve, reject) => {
    if (typeof window === 'undefined' || typeof document === 'undefined') {
      reject(new Error('Wasm preview renderer can only load in a browser runtime.'))
      return
    }

    if (window.__ovrleyWasmPreviewModule?.calledRun) {
      resolve(window.__ovrleyWasmPreviewModule)
      return
    }

    const moduleConfig = {
      locateFile(path) {
        return path.endsWith('.wasm') ? WASM_URL : `${ARTIFACT_BASE_URL}/${path}`
      },
      print(message) {
        console.info('[wasm-preview]', message)
      },
      printErr(message) {
        console.error('[wasm-preview]', message)
      },
      onAbort(reason) {
        reject(new Error(`Wasm runtime aborted: ${reason}`))
      },
      onRuntimeInitialized() {
        window.__ovrleyWasmPreviewModule = window.Module
        resolve(window.Module)
      },
    }

    window.Module = moduleConfig

    const script = document.createElement('script')
    script.src = `${SCRIPT_URL}?t=${Date.now()}`
    script.async = true
    script.dataset.ovrleyWasmPreview = 'true'
    script.onload = () => {
      if (window.Module?.calledRun) {
        window.__ovrleyWasmPreviewModule = window.Module
        resolve(window.Module)
      }
    }
    script.onerror = () => {
      reject(new Error(`Failed to load ${SCRIPT_URL}. Run "pnpm wasm:preview:build" and start the Vite/Tauri dev server.`))
    }

    document.head.appendChild(script)
  }).catch((error) => {
    modulePromise = null
    throw error
  })

  return modulePromise
}

/**
 * Loads a font from bytes into the Wasm renderer.
 *
 * @param {ArrayBuffer} fontBytes - Font file bytes.
 * @returns {Promise<boolean>} True if font loaded successfully.
 */
export async function loadWasmFontFromBytes(fontBytes) {
  if (fontLoaded) {
    return true
  }

  const module = await loadWasmPreviewModule()
  const ptr = module._ovrley_wasm_preview_alloc(fontBytes.byteLength)
  if (!ptr) {
    throw new Error(`Font allocation failed for ${fontBytes.byteLength} bytes.`)
  }

  try {
    module.HEAPU8.set(new Uint8Array(fontBytes), ptr)
    const result = module._ovrley_wasm_preview_load_font_from_bytes(ptr, fontBytes.byteLength)
    if (result !== 0) {
      throw new Error(`Font loading failed with error code ${result}.`)
    }
    fontLoaded = true
    return true
  } finally {
    module._ovrley_wasm_preview_dealloc(ptr, fontBytes.byteLength)
  }
}

/**
 * Renders a dynamic text widget with a changing numeric value and optional unit label.
 *
 * All buffer allocations are persistent and reused across frames to avoid
 * per-frame alloc/dealloc overhead (the main bottleneck in the previous version).
 *
 * @param {HTMLCanvasElement} canvas - Canvas that hosts the 4K drawing surface.
 * @param {string} value - Numeric value to display.
 * @param {string} unit - Optional unit label.
 * @returns {object} Render metadata including per-phase timing breakdown.
 */
export function renderDynamicTextWidgetSync(canvas, value, unit = '') {
  if (!modulePromise) {
    throw new Error('Wasm module not loaded.')
  }

  // Use the already-resolved module (synchronous fast path)
  const module = /** @type {any} */ (window.__ovrleyWasmPreviewModule)
  if (!module?.calledRun) {
    throw new Error('Wasm module not initialized.')
  }

  const t0 = performance.now()

  const width = module._ovrley_wasm_preview_width()
  const height = module._ovrley_wasm_preview_height()
  const rgbaLen = module._ovrley_wasm_preview_rgba_len()
  const backendCode = module._ovrley_wasm_preview_backend()

  // Ensure persistent RGBA buffer is allocated
  ensureRgbaBuffer(module, rgbaLen)

  // Encode strings using the shared TextEncoder (no per-frame allocation)
  const valueBytes = textEncoder.encode(value)
  const unitBytes = textEncoder.encode(unit)

  // Ensure persistent string buffers are large enough
  const valBuf = ensureStringBuffer(module, valueBytes.byteLength || 1, 'value')
  const unitBuf = ensureStringBuffer(module, unitBytes.byteLength || 1, 'unit')

  // Copy string data into persistent Wasm buffers
  if (valueBytes.byteLength > 0) {
    module.HEAPU8.set(valueBytes, valBuf.ptr)
  }
  if (unitBytes.byteLength > 0) {
    module.HEAPU8.set(unitBytes, unitBuf.ptr)
  }

  const t1 = performance.now()

  // Render the frame into the persistent RGBA buffer (Skia software rasterization)
  const result = module._ovrley_wasm_preview_render_dynamic_text_widget(
    persistentRgbaPtr,
    persistentRgbaLen,
    valBuf.ptr,
    valueBytes.byteLength,
    unitBuf.ptr,
    unitBytes.byteLength,
  )

  const t2 = performance.now()

  if (result !== 0) {
    throw new Error(`Wasm renderer returned error code ${result}.`)
  }

  // Paint to canvas — reuse per-canvas context and ImageData to avoid
  // per-frame overhead even when benchmarking multiple widget surfaces.
  const { context, imageData } = getCanvasResources(canvas, width, height)

  // Copy pixels into the reusable ImageData buffer
  imageData.data.set(new Uint8ClampedArray(module.HEAPU8.buffer, persistentRgbaPtr, persistentRgbaLen))
  const t3 = performance.now()

  context.putImageData(imageData, 0, 0)
  const t4 = performance.now()

  return {
    backendLabel: BACKEND_LABELS[backendCode] || `backend-${backendCode}`,
    byteLength: rgbaLen,
    height,
    width,
    value,
    unit,
    timing: {
      prepare: Math.round((t1 - t0) * 100) / 100,
      wasmDraw: Math.round((t2 - t1) * 100) / 100,
      bufferCopy: Math.round((t3 - t2) * 100) / 100,
      putImage: Math.round((t4 - t3) * 100) / 100,
      total: Math.round((t4 - t0) * 100) / 100,
    },
  }
}

/**
 * Renders a dynamic text widget (async wrapper kept for call-site compatibility).
 *
 * @param {HTMLCanvasElement} canvas - Canvas that hosts the 4K drawing surface.
 * @param {string} value - Numeric value to display.
 * @param {string} unit - Optional unit label.
 * @returns {Promise<object>} Render metadata.
 */
export async function renderDynamicTextWidget(canvas, value, unit = '') {
  if (!modulePromise) {
    await loadWasmPreviewModule()
  }
  return renderDynamicTextWidgetSync(canvas, value, unit)
}

/**
 * Draws the renderer's static frame into the supplied canvas.
 *
 * @param {HTMLCanvasElement} canvas - Canvas that hosts the 4K drawing surface.
 * @returns {Promise<object>} Render metadata.
 */
export async function drawWasmPreviewStaticFrame(canvas) {
  const module = await loadWasmPreviewModule()
  const width = module._ovrley_wasm_preview_width()
  const height = module._ovrley_wasm_preview_height()
  const rgbaLen = module._ovrley_wasm_preview_rgba_len()
  const backendCode = module._ovrley_wasm_preview_backend()
  const ptr = module._ovrley_wasm_preview_alloc(rgbaLen)

  if (!ptr) {
    throw new Error(`Wasm renderer allocation failed for ${rgbaLen} bytes.`)
  }

  try {
    const result = module._ovrley_wasm_preview_render_static_frame(ptr, rgbaLen)
    if (result !== 0) {
      throw new Error(`Wasm renderer returned error code ${result}.`)
    }

    canvas.width = width
    canvas.height = height

    const context = canvas.getContext('2d')
    if (!context) {
      throw new Error('Browser did not provide a 2D canvas context.')
    }

    const pixels = new Uint8ClampedArray(module.HEAPU8.slice(ptr, ptr + rgbaLen))
    context.putImageData(new ImageData(pixels, width, height), 0, 0)

    return {
      backendLabel: BACKEND_LABELS[backendCode] || `backend-${backendCode}`,
      byteLength: rgbaLen,
      height,
      width,
    }
  } catch (error) {
    throw new Error(describeError(error))
  } finally {
    module._ovrley_wasm_preview_dealloc(ptr, rgbaLen)
  }
}
