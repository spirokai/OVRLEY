const BASE_URL = '/debug/wasm-bindings'

/**
 * Create an independently owned renderer and persistent RGBA buffer.
 * @param {HTMLCanvasElement} canvas Destination canvas.
 * @returns {Promise<{render: Function, dispose: Function}>} Renderer lifecycle.
 */
export async function createWasmRenderer(canvas) {
  const url = `${BASE_URL}/wasm_bindings.js`
  const { default: createModule } = await import(/* @vite-ignore */ url)
  const module = await createModule({ locateFile: (name) => `${BASE_URL}/${name}` })
  if (module._ovrley_abi_version() !== 1) throw new Error('Unsupported Wasm renderer ABI.')
  const response = await fetch(`${BASE_URL}/font.ttf`)
  if (!response.ok) throw new Error(`Font request failed (${response.status}).`)
  const font = new Uint8Array(await response.arrayBuffer())
  const upload = module._ovrley_alloc(font.byteLength)
  if (!upload) throw new Error('Font allocation failed.')
  let handle
  try {
    module.HEAPU8.set(font, upload)
    handle = module._ovrley_renderer_create(upload, font.byteLength)
  } finally {
    module._ovrley_dealloc(upload, font.byteLength)
  }
  if (!handle) throw new Error('Skia rejected the font.')
  let pixels = 0
  let disposed = false
  function dispose() {
    if (disposed) return
    disposed = true
    if (pixels) module._ovrley_dealloc(pixels, module._ovrley_rgba_len())
    module._ovrley_renderer_destroy(handle)
  }
  try {
    canvas.width = module._ovrley_width()
    canvas.height = module._ovrley_height()
    const length = module._ovrley_rgba_len()
    if (length !== canvas.width * canvas.height * 4) throw new Error('Invalid RGBA surface dimensions.')
    const context = canvas.getContext('2d')
    if (!context) throw new Error('Canvas 2D is unavailable.')
    const image = new ImageData(canvas.width, canvas.height)
    pixels = module._ovrley_alloc(length)
    if (!pixels) throw new Error('Pixel allocation failed.')
    return {
      dispose,
      render(frame) {
        if (disposed) throw new Error('Renderer is disposed.')
        const start = performance.now()
        const status = module._ovrley_render_frame(handle, pixels, length, frame)
        const drawn = performance.now()
        if (status !== 0) throw new Error(`Skia render failed (${status}).`)
        image.data.set(module.HEAPU8.subarray(pixels, pixels + length))
        const copied = performance.now()
        context.putImageData(image, 0, 0)
        const presented = performance.now()
        return { draw: drawn - start, copy: copied - drawn, present: presented - copied, total: presented - start }
      },
    }
  } catch (error) {
    dispose()
    throw error
  }
}
