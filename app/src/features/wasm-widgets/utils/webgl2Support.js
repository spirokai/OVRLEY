/** WebGL2 support for the map preview, which draws on WebGL2. Probed once per app. */

/** Software rasterizers a webview falls back to without a GPU: WARP, Mesa's llvmpipe and softpipe, SwiftShader. */
const SOFTWARE_RENDERER = /basic render driver|llvmpipe|softpipe|swiftshader/i

let support

function probe() {
  const gl = document.createElement('canvas').getContext('webgl2')
  if (!gl) return 'unavailable'
  // Without the extension the renderer is unknown, and counts as a GPU.
  const info = gl.getExtension('WEBGL_debug_renderer_info')
  const renderer = info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : ''
  gl.getExtension('WEBGL_lose_context')?.loseContext()
  return SOFTWARE_RENDERER.test(renderer) ? 'software' : 'hardware'
}

/**
 * Export draws maps natively and needs a GPU, so anything but `hardware` means it cannot draw them.
 * @returns {'hardware'|'software'|'unavailable'} How this webview draws WebGL2.
 */
export function getWebGl2Support() {
  support ??= probe()
  return support
}
