import { clamp } from '@/lib/utils'

/** @param {MouseEvent} event Pointer event. @returns {boolean} Whether to extend or toggle the selection. */
export function hasSelectionModifier(event) {
  return event.metaKey || event.ctrlKey || event.shiftKey
}

/** @param {object} start First local point. @param {object} end Second local point. @returns {object} Normalized selection rectangle. */
export function buildSelectionRect(start, end) {
  return { x: Math.min(start.x, end.x), y: Math.min(start.y, end.y), width: Math.abs(end.x - start.x), height: Math.abs(end.y - start.y) }
}

function rectanglesIntersect(first, second) {
  return !(
    first.x + first.width < second.x ||
    second.x + second.width < first.x ||
    first.y + first.height < second.y ||
    second.y + second.height < first.y
  )
}

function getElementScale(element, bounds) {
  const width = element.offsetWidth || element.clientWidth
  const height = element.offsetHeight || element.clientHeight
  return { x: width > 0 ? bounds.width / width : 1, y: height > 0 ? bounds.height / height : 1 }
}

/** @param {HTMLElement} element Reference element. @param {number} clientX Client X. @param {number} clientY Client Y. @returns {object} Local point. */
export function getElementPoint(element, clientX, clientY) {
  const bounds = element.getBoundingClientRect()
  const scale = getElementScale(element, bounds)
  return { x: (clientX - bounds.left) / scale.x, y: (clientY - bounds.top) / scale.y }
}

/** @param {object} context Scene element and dimensions. @param {MouseEvent} event Pointer event. @returns {object} Point clamped to the scene. */
export function getScenePoint({ sceneElement, sceneSize }, event) {
  const point = getElementPoint(sceneElement, event.clientX, event.clientY)
  return { x: clamp(point.x, 0, sceneSize.width), y: clamp(point.y, 0, sceneSize.height) }
}

/** @param {object} context Scene, widget order, and registered nodes. @param {object} rect Scene selection rectangle. @returns {string[]} Intersected IDs. */
export function getIntersectedWidgetIds({ orderedWidgetIds, sceneElement, widgetNodes }, rect) {
  const sceneBounds = sceneElement.getBoundingClientRect()
  const scale = getElementScale(sceneElement, sceneBounds)
  const ids = []
  for (const id of orderedWidgetIds) {
    const node = widgetNodes[id]
    if (!node) continue // Wrappers can be unmounted while selection is reconciled.
    const bounds = node.getBoundingClientRect()
    if (
      rectanglesIntersect(rect, {
        x: (bounds.left - sceneBounds.left) / scale.x,
        y: (bounds.top - sceneBounds.top) / scale.y,
        width: bounds.width / scale.x,
        height: bounds.height / scale.y,
      })
    ) {
      ids.push(id)
    }
  }
  return ids
}

/** @param {object} context Visible selection, hovered ID, viewport scale, and mounted WASM wrappers. @returns {object[]} Badge positions above content bounds. */
export function buildWidgetBadges({ displayScale, hoveredWidgetId, orderedWidgetIds, sceneElement, selectedWidgetIds, widgetMap, widgetNodes }) {
  if (sceneElement === null) return []
  const visibleIds = new Set(selectedWidgetIds)
  if (hoveredWidgetId !== null) visibleIds.add(hoveredWidgetId)
  const sceneBounds = sceneElement.getBoundingClientRect()
  const scale = getElementScale(sceneElement, sceneBounds)
  const badges = []
  for (const id of orderedWidgetIds) {
    const node = widgetNodes[id]
    if (!visibleIds.has(id) || !node) continue
    const bounds = node.getBoundingClientRect()
    badges.push({
      id,
      type: widgetMap[id].type,
      left: ((bounds.left - sceneBounds.left) / scale.x) * displayScale - 2,
      top: Math.max(((bounds.top - sceneBounds.top) / scale.y) * displayScale - 20, 0),
    })
  }
  return badges
}

/** @param {object[]} previous Current badge presentation. @param {object[]} next Measured badge presentation. @returns {boolean} Whether another render is unnecessary. */
export function areWidgetBadgesEqual(previous, next) {
  if (previous.length !== next.length) return false
  for (let index = 0; index < previous.length; index++) {
    const left = previous[index]
    const right = next[index]
    if (left.id !== right.id || left.type !== right.type || left.left !== right.left || left.top !== right.top) return false
  }
  return true
}
