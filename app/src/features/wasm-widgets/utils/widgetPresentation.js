/**
 * Installs raster placement and tight interaction bounds together. Both rectangles
 * are supplied by Rust in scene pixels; canvas overflow preserves shadows.
 * @param {HTMLCanvasElement} canvas Widget raster.
 * @param {object} geometry Rust drawing and content bounds.
 */
export function presentWidgetGeometry(canvas, geometry) {
  const target = canvas.parentElement
  const { content } = geometry
  target.style.left = `${content.left}px`
  target.style.top = `${content.top}px`
  target.style.width = `${content.width}px`
  target.style.height = `${content.height}px`
  target.style.visibility = 'visible'
  target.dataset.widgetContentWidth = geometry.content_width
  canvas.style.left = `${geometry.left - content.left}px`
  canvas.style.top = `${geometry.top - content.top}px`
  target.dispatchEvent(new Event('wasm-widget-render', { bubbles: true }))
}
