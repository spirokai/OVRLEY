/**
 * Installs Rust placement and interaction bounds. Static image buffers retain
 * source dimensions and are displayed at their scene size; other rasters overflow for shadows.
 * @param {HTMLCanvasElement} canvas Widget raster.
 * @param {object} geometry Rust drawing and content bounds.
 * @param {HTMLElement} target Selection wrapper; a sibling raster avoids transformed overflow for routes.
 * @param {string} widgetType Canonical widget family.
 */
export function presentWidgetGeometry(canvas, geometry, target = canvas.parentElement, widgetType) {
  const { content } = geometry
  const sourceRaster = widgetType === 'raster'
  // Moveable reads offsetLeft/offsetTop in whole CSS pixels but sizes and transforms fractionally.
  // Whole-pixel placement plus a fractional translate keeps its box exactly on the content.
  const left = Math.floor(content.left)
  const top = Math.floor(content.top)
  target.style.left = `${left}px`
  target.style.top = `${top}px`
  target.style.width = `${content.width}px`
  target.style.height = `${content.height}px`
  target.style.visibility = 'visible'
  target.dataset.widgetContentWidth = geometry.content_width
  const nested = canvas.parentElement === target
  canvas.style.left = `${geometry.left - (nested ? content.left : 0)}px`
  canvas.style.top = `${geometry.top - (nested ? content.top : 0)}px`
  // Density changes raster allocation; placement and interaction stay in scene pixels.
  const rasterScale = geometry.preview_raster_scale
  canvas.style.width = `${sourceRaster ? content.width : geometry.width / rasterScale}px`
  canvas.style.height = `${sourceRaster ? content.height : geometry.height / rasterScale}px`
  canvas.style.opacity = sourceRaster ? `${geometry.opacity}` : ''
  // Other rotated families already have their rotation baked into the pixels.
  const rotation = geometry.rotation ?? 0
  const origin = geometry.rotation_origin ?? [0, 0]
  target.style.transformOrigin = `${origin[0]}px ${origin[1]}px`
  target.style.transform = `translate(${content.left - left}px, ${content.top - top}px) rotate(${rotation}deg)`
  canvas.style.transformOrigin = `${content.left - geometry.left + origin[0]}px ${content.top - geometry.top + origin[1]}px`
  canvas.style.transform = nested && !sourceRaster ? `rotate(${-rotation}deg)` : 'none'
  target.dispatchEvent(new Event('wasm-widget-render', { bubbles: true }))
}
