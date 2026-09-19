const COURSE_SNAP_DISTANCE_PIXELS = 40

/**
 * Converts canonical course segments into the GeoJSON shape consumed by MapLibre.
 *
 * @param {{coordinate: number[], activitySecond: number}[][]} courseSegments Timed course segments.
 * @returns {object} GeoJSON multi-line feature.
 */
export function createCourseGeoJson(courseSegments) {
  return {
    type: 'Feature',
    properties: {},
    geometry: { type: 'MultiLineString', coordinates: courseSegments.map((segment) => segment.map((point) => point.coordinate)) },
  }
}

function closestPointOnSegment(cursor, start, end) {
  const deltaX = end.x - start.x
  const deltaY = end.y - start.y
  const lengthSquared = deltaX * deltaX + deltaY * deltaY
  const progress = lengthSquared === 0 ? 0 : Math.max(0, Math.min(1, ((cursor.x - start.x) * deltaX + (cursor.y - start.y) * deltaY) / lengthSquared))
  const point = { x: start.x + progress * deltaX, y: start.y + progress * deltaY }
  return { point, progress, distanceSquared: (cursor.x - point.x) ** 2 + (cursor.y - point.y) ** 2 }
}

/**
 * Finds the closest course position to a cursor in rendered pixel space.
 *
 * @param {object} map MapLibre map projection interface.
 * @param {{x: number, y: number}} cursorPoint Cursor position in map pixels.
 * @param {{coordinate: number[], activitySecond: number}[][]} courseSegments Timed course segments.
 * @returns {{position: object, activitySecond: number}|null} Snapped position and interpolated activity time.
 */
export function getSnappedCoursePosition(map, cursorPoint, courseSegments) {
  let closest = null
  for (const segment of courseSegments) {
    for (let index = 1; index < segment.length; index += 1) {
      const start = segment[index - 1]
      const end = segment[index]
      const candidate = closestPointOnSegment(cursorPoint, map.project(start.coordinate), map.project(end.coordinate))
      if (!closest || candidate.distanceSquared < closest.distanceSquared) {
        closest = {
          ...candidate,
          activitySecond: start.activitySecond + candidate.progress * (end.activitySecond - start.activitySecond),
        }
      }
    }
  }
  if (!closest || closest.distanceSquared > COURSE_SNAP_DISTANCE_PIXELS ** 2) return null
  return { position: map.unproject(closest.point), activitySecond: closest.activitySecond }
}

/**
 * Interpolates the course coordinate for one detected activity time.
 *
 * @param {{coordinate: number[], activitySecond: number}[][]} courseSegments Timed course segments.
 * @param {number} activitySecond Detected activity time.
 * @returns {number[]|null} Longitude/latitude coordinate, or null when the time is outside the mapped course.
 */
export function getCoursePositionAtActivitySecond(courseSegments, activitySecond) {
  for (const segment of courseSegments) {
    for (let index = 1; index < segment.length; index += 1) {
      const start = segment[index - 1]
      const end = segment[index]
      if (activitySecond < start.activitySecond || activitySecond > end.activitySecond) continue

      const duration = end.activitySecond - start.activitySecond
      const progress = duration === 0 ? 0 : (activitySecond - start.activitySecond) / duration
      return [
        start.coordinate[0] + progress * (end.coordinate[0] - start.coordinate[0]),
        start.coordinate[1] + progress * (end.coordinate[1] - start.coordinate[1]),
      ]
    }
  }
  return null
}
