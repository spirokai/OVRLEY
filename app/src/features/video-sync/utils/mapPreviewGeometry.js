import { VIDEO_SYNC_MAP_SNAP_DISTANCE_PIXELS } from '../data/videoSyncConstants'

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
  if (!closest || closest.distanceSquared > VIDEO_SYNC_MAP_SNAP_DISTANCE_PIXELS ** 2) return null
  return { position: map.unproject(closest.point), activitySecond: closest.activitySecond }
}

function findCoursePosition(courseSegments, activitySecond) {
  for (const segment of courseSegments) {
    for (let index = 1; index < segment.length; index += 1) {
      const start = segment[index - 1]
      const end = segment[index]
      if (activitySecond < start.activitySecond || activitySecond > end.activitySecond) continue
      const duration = end.activitySecond - start.activitySecond
      const progress = duration === 0 ? 0 : (activitySecond - start.activitySecond) / duration
      return { segment, index, coordinate: interpolateCoordinate(start.coordinate, end.coordinate, progress) }
    }
  }
  return null
}

/**
 * Interpolates the course coordinate for one detected activity time.
 *
 * @param {{coordinate: number[], activitySecond: number}[][]} courseSegments Timed course segments.
 * @param {number} activitySecond Detected activity time.
 * @returns {number[]|null} Longitude/latitude coordinate, or null when the time is outside the mapped course.
 */
export function getCoursePositionAtActivitySecond(courseSegments, activitySecond) {
  return findCoursePosition(courseSegments, activitySecond)?.coordinate ?? null
}

const EARTH_RADIUS_METERS = 6_371_000

function toRadians(degrees) {
  return (degrees * Math.PI) / 180
}

function coordinateDistanceMeters(start, end) {
  const startLatitude = toRadians(start[1])
  const endLatitude = toRadians(end[1])
  const latitudeDelta = endLatitude - startLatitude
  const longitudeDelta = toRadians(end[0] - start[0])
  const haversine = Math.sin(latitudeDelta / 2) ** 2 + Math.cos(startLatitude) * Math.cos(endLatitude) * Math.sin(longitudeDelta / 2) ** 2
  return 2 * EARTH_RADIUS_METERS * Math.atan2(Math.sqrt(haversine), Math.sqrt(1 - haversine))
}

function coordinateBearing(start, end) {
  const startLatitude = toRadians(start[1])
  const endLatitude = toRadians(end[1])
  const longitudeDelta = toRadians(end[0] - start[0])
  const y = Math.sin(longitudeDelta) * Math.cos(endLatitude)
  const x = Math.cos(startLatitude) * Math.sin(endLatitude) - Math.sin(startLatitude) * Math.cos(endLatitude) * Math.cos(longitudeDelta)
  return (Math.atan2(y, x) * 180) / Math.PI
}

function interpolateCoordinate(start, end, progress) {
  return [start[0] + progress * (end[0] - start[0]), start[1] + progress * (end[1] - start[1])]
}

/**
 * Resolves a navigation camera from timed course geometry. The bearing points
 * toward a fixed-distance lookahead point, which smooths vertex-to-vertex
 * direction changes without consulting the recorded heading metric.
 *
 * @param {{coordinate: number[], activitySecond: number}[][]} courseSegments Timed course segments.
 * @param {number} activitySecond Current activity preview time.
 * @param {number} lookaheadMeters Route distance used to estimate forward bearing.
 * @returns {{center: number[], bearing: number|null}|null} Navigation camera, or null outside the course.
 */
export function getCourseNavigationCamera(courseSegments, activitySecond, lookaheadMeters) {
  const position = findCoursePosition(courseSegments, activitySecond)
  if (position === null) return null
  const { segment, index, coordinate: center } = position
  let remainingDistance = lookaheadMeters
  let cursor = center

  for (let routeIndex = index; routeIndex < segment.length; routeIndex += 1) {
    const target = segment[routeIndex].coordinate
    const distance = coordinateDistanceMeters(cursor, target)
    if (distance >= remainingDistance && distance > 0) {
      const lookahead = interpolateCoordinate(cursor, target, remainingDistance / distance)
      return { center, bearing: coordinateBearing(center, lookahead) }
    }
    remainingDistance -= distance
    cursor = target
  }

  return { center, bearing: coordinateDistanceMeters(center, cursor) > 0 ? coordinateBearing(center, cursor) : null }
}
