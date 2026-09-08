import { buildWidgetTransform } from '@/lib/geometryUtils'
import { isFramedWidget } from '@/lib/widget/display-type-behavior'

function readStyleLength(target, property, fallback) {
  const value = Number.parseFloat(target?.style?.[property])
  if (Number.isFinite(value)) return value

  const offsetProperty = property === 'width' ? 'offsetWidth' : 'offsetHeight'
  const offsetValue = target?.[offsetProperty]
  if (Number.isFinite(offsetValue) && offsetValue > 0) return offsetValue

  return fallback
}

function readStylePosition(target, property, fallback) {
  const value = Number.parseFloat(target?.style?.[property])
  return Number.isFinite(value) ? value : fallback
}

/**
 * Captures the target layout once at interaction start.
 *
 * @param {HTMLElement|null} target - Widget DOM node.
 * @param {object} widget - Widget definition.
 * @param {number} globalScale - Scene scale applied to framed widgets.
 * @returns {{left: number, top: number, width: number, height: number, rotation: number, transform: string}}
 */
export function captureWidgetLayout(target, widget, globalScale) {
  const data = widget.data
  return {
    left: readStylePosition(target, 'left', data.x ?? 0),
    top: readStylePosition(target, 'top', data.y ?? 0),
    width: readStyleLength(target, 'width', (data.width ?? 0) * (globalScale || 1)),
    height: readStyleLength(target, 'height', (data.height ?? 0) * (globalScale || 1)),
    rotation: data.rotation ?? 0,
    transform: target?.style?.transform || '',
  }
}

/**
 * Returns the position used as a Moveable interaction origin.
 *
 * @param {object} widget - Widget definition.
 * @param {object} layout - Captured target layout.
 * @returns {{x: number, y: number}} Interaction data origin.
 */
export function getWidgetInteractionPosition(widget, layout) {
  if (isFramedWidget(widget)) {
    return { x: layout.left, y: layout.top }
  }

  return {
    x: widget.data.x ?? 0,
    y: widget.data.y ?? 0,
  }
}

/**
 * Builds a live layout for a frame resize from the captured origin.
 *
 * @param {object} originLayout - Layout captured before the interaction.
 * @param {object} options - Resize layout values.
 * @returns {object} Live frame layout.
 */
export function buildFrameInteractionLayout(originLayout, { width, height, translateX = 0, translateY = 0, rotation = originLayout.rotation ?? 0 }) {
  return {
    mode: 'frame',
    left: originLayout.left + translateX,
    top: originLayout.top + translateY,
    width,
    height,
    rotation,
  }
}

/**
 * Builds a live layout for intrinsic scaling from the captured origin.
 *
 * @param {object} originLayout - Layout captured before the interaction.
 * @param {object} options - Scale layout values.
 * @returns {object} Live scale layout.
 */
export function buildScaleInteractionLayout(
  originLayout,
  { scaleFactor, globalScale, translateX = 0, translateY = 0, rotation = 0, transformOriginX = 0, left = originLayout.left },
) {
  return {
    mode: 'scale',
    left,
    top: originLayout.top,
    width: originLayout.width,
    height: originLayout.height,
    scaleFactor,
    globalScale,
    translateX,
    translateY,
    transformOriginX,
    rotation,
  }
}

/**
 * Resolves the CSS transform for a live interaction layout.
 *
 * @param {object} layout - Live interaction layout.
 * @param {number} globalScale - Scene scale.
 * @returns {string} CSS transform string.
 */
export function getLiveWidgetTransform(layout, globalScale) {
  if (!layout) return ''

  if (layout.mode === 'drag') return layout.transform || ''

  if (layout.mode !== 'scale') {
    return buildWidgetTransform({ rotation: layout.rotation || 0 }) || ''
  }

  const translate = layout.translateX || layout.translateY ? `translate(${layout.translateX || 0}px, ${layout.translateY || 0}px)` : ''
  const scale = buildWidgetTransform({ scale: (globalScale || 1) * layout.scaleFactor, rotation: layout.rotation || 0 }) || ''
  return [translate, scale].filter(Boolean).join(' ')
}

/**
 * Builds a translated live layout for drag interactions.
 *
 * @param {object} originLayout - Layout captured before the interaction.
 * @param {number} translateX - Horizontal translation.
 * @param {number} translateY - Vertical translation.
 * @returns {object} Live translated layout.
 */
export function buildDragInteractionLayout(originLayout, translateX, translateY) {
  return {
    ...originLayout,
    mode: 'drag',
    left: originLayout.left + translateX,
    top: originLayout.top + translateY,
  }
}

/**
 * Builds a live layout for rotation from the captured origin.
 *
 * @param {object} originLayout - Layout captured before the interaction.
 * @param {number} translateX - Horizontal translation.
 * @param {number} translateY - Vertical translation.
 * @param {number} rotation - Rotation in degrees.
 * @returns {object} Live rotated layout.
 */
export function buildRotateInteractionLayout(originLayout, translateX, translateY, rotation) {
  return {
    ...originLayout,
    mode: 'frame',
    left: originLayout.left + translateX,
    top: originLayout.top + translateY,
    transform: buildWidgetTransform({ rotation }) || '',
  }
}
