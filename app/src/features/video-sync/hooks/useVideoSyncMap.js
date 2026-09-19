import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { LngLatBounds, Map, Marker, NavigationControl } from 'maplibre-gl'
import { getMapStyleUrlTemplate } from '@/api/backend'
import { getPreference, setPreference } from '@/lib/preferences-store'
import {
  VIDEO_SYNC_DEFAULT_MAP_STYLE,
  VIDEO_SYNC_MAP_INITIAL_CENTER,
  VIDEO_SYNC_MAP_INITIAL_ZOOM,
  VIDEO_SYNC_MAP_RESIZE_SETTLE_DELAY_MS,
  VIDEO_SYNC_MAP_STYLES,
  VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY,
} from '../data/videoSyncConstants'
import { buildActivityCourseSegments } from '../utils/activitySyncInput'
import { createCourseGeoJson, getCoursePositionAtActivitySecond, getSnappedCoursePosition } from '../utils/mapPreviewGeometry'

const COURSE_SOURCE_ID = 'activity-course'
const COURSE_LAYER_ID = 'activity-course-line'
const EMPTY_STYLE = { version: 8, sources: {}, layers: [] }

function requireMapStyle(value) {
  if (!VIDEO_SYNC_MAP_STYLES.includes(value)) {
    throw new Error(`Preference "${VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY}" must be a supported map style`)
  }
  return value
}

class VideoSyncMapController {
  constructor(onActionPointChange, onError) {
    this.onActionPointChange = onActionPointChange
    this.onError = onError
    this.courseSegments = []
    this.canvasContainer = null
    this.detection = null
    this.detectedLocationMarker = null
    this.map = null
    this.hoverMarker = null
    this.actionLocation = null
    this.hasFittedCourse = false
    this.style = VIDEO_SYNC_DEFAULT_MAP_STYLE
    this.styleUrlTemplate = null
    this.styleLoaded = false
    this.styleLoadPending = false
    this.resizeObserver = null
    this.resizeTimeout = null

    this.handleClick = this.handleClick.bind(this)
    this.handleMouseMove = this.handleMouseMove.bind(this)
    this.handleMouseOut = this.handleMouseOut.bind(this)
    this.handleMove = this.handleMove.bind(this)
    this.handleResize = this.handleResize.bind(this)
    this.handleStyleLoad = this.handleStyleLoad.bind(this)
  }

  mount(container) {
    this.map = new Map({
      container,
      style: EMPTY_STYLE,
      center: VIDEO_SYNC_MAP_INITIAL_CENTER,
      zoom: VIDEO_SYNC_MAP_INITIAL_ZOOM,
      attributionControl: true,
    })
    this.canvasContainer = this.map.getCanvasContainer()
    this.map.addControl(new NavigationControl({ showCompass: false }), 'top-right')
    this.map.on('click', this.handleClick)
    this.map.on('mousemove', this.handleMouseMove)
    this.map.on('mouseout', this.handleMouseOut)
    this.map.on('move', this.handleMove)
    this.map.on('style.load', this.handleStyleLoad)
    this.resizeObserver = new ResizeObserver(this.handleResize)
    this.resizeObserver.observe(container)
    this.loadStyle()
  }

  async loadStyle() {
    try {
      this.styleUrlTemplate = await getMapStyleUrlTemplate()
    } catch (error) {
      if (this.map) this.onError(error)
      return
    }
    if (this.map && this.styleUrlTemplate) {
      this.applyStyle()
    }
  }

  dispose() {
    this.resizeObserver?.disconnect()
    this.resizeObserver = null
    if (this.resizeTimeout !== null) window.clearTimeout(this.resizeTimeout)
    this.resizeTimeout = null
    this.syncHoverTarget(null)
    this.detectedLocationMarker?.remove()
    this.detectedLocationMarker = null
    this.canvasContainer = null
    this.map?.remove()
    this.map = null
  }

  getStyleUrl() {
    return this.styleUrlTemplate?.replace('{style}', this.style) ?? null
  }

  setStyle(style) {
    this.style = style
    this.applyStyle()
  }

  applyStyle() {
    if (!this.map || !this.styleUrlTemplate) return
    this.styleLoaded = false
    this.styleLoadPending = true
    this.map.setStyle(this.getStyleUrl())
  }

  setCourseSegments(courseSegments) {
    this.courseSegments = courseSegments
    if (this.styleLoaded) this.syncCourse()
    this.syncDetectedLocationMarker()
  }

  setDetection(detection) {
    this.detection = detection
    this.syncDetectedLocationMarker()
  }

  resize() {
    if (!this.map) return
    this.map.resize()
  }

  handleResize() {
    if (this.resizeTimeout !== null) window.clearTimeout(this.resizeTimeout)
    this.resizeTimeout = window.setTimeout(() => {
      this.resizeTimeout = null
      this.resize()
    }, VIDEO_SYNC_MAP_RESIZE_SETTLE_DELAY_MS)
  }

  handleStyleLoad() {
    if (!this.styleLoadPending) return
    this.styleLoadPending = false
    this.styleLoaded = true
    this.syncCourse()
  }

  syncCourse() {
    const data = createCourseGeoJson(this.courseSegments)
    const source = this.map.getSource(COURSE_SOURCE_ID)
    if (source) {
      source.setData(data)
    } else {
      this.map.addSource(COURSE_SOURCE_ID, { type: 'geojson', data })
      this.map.addLayer({
        id: COURSE_LAYER_ID,
        type: 'line',
        source: COURSE_SOURCE_ID,
        layout: { 'line-cap': 'round', 'line-join': 'round' },
        paint: { 'line-color': '#EF6C15', 'line-opacity': 1, 'line-width': 6 },
      })
    }
    this.fitCourse()
  }

  fitCourse() {
    if (this.hasFittedCourse || this.courseSegments.length === 0) return
    const bounds = new LngLatBounds()
    for (const segment of this.courseSegments) {
      for (const point of segment) bounds.extend(point.coordinate)
    }
    this.map.fitBounds(bounds, { animate: false, maxZoom: 17, padding: 64 })
    this.hasFittedCourse = true
  }

  handleMouseMove(event) {
    this.syncHoverTarget(getSnappedCoursePosition(this.map, event.point, this.courseSegments))
  }

  handleMouseOut() {
    this.syncHoverTarget(null)
  }

  syncHoverTarget(location) {
    this.canvasContainer.style.cursor = location === null ? '' : 'crosshair'

    if (location === null) {
      this.hoverMarker?.remove()
      this.hoverMarker = null
    } else if (this.hoverMarker === null) {
      const element = document.createElement('div')
      element.className = 'pointer-events-none size-4 rounded-full border border-white bg-video-sync-location'
      this.hoverMarker = new Marker({ element }).setLngLat(location.position).addTo(this.map)
    } else {
      this.hoverMarker.setLngLat(location.position)
    }
  }

  syncDetectedLocationMarker() {
    const location = this.detection?.location ?? null
    const coordinate = location === null ? null : getCoursePositionAtActivitySecond(this.courseSegments, location.time)
    if (coordinate === null) {
      this.detectedLocationMarker?.remove()
      this.detectedLocationMarker = null
      return
    }

    if (this.detectedLocationMarker === null) {
      this.detectedLocationMarker = new Marker({ color: 'var(--color-video-sync-location)', scale: 1 }).setLngLat(coordinate).addTo(this.map)
      return
    }

    this.detectedLocationMarker.setLngLat(coordinate)
  }

  handleClick(event) {
    this.actionLocation = getSnappedCoursePosition(this.map, event.point, this.courseSegments)
    this.updateActionPoint()
  }

  handleMove() {
    this.updateActionPoint()
  }

  clearActionLocation() {
    this.actionLocation = null
    this.updateActionPoint()
  }

  updateActionPoint() {
    if (!this.actionLocation) {
      this.onActionPointChange(null)
      return
    }
    const point = this.map.project(this.actionLocation.position)
    this.onActionPointChange({ ...point, activitySecond: this.actionLocation.activitySecond })
  }
}

/**
 * Owns the complete MapLibre preview lifecycle and exposes only presentation state.
 *
 * @param {object} options Map preview inputs.
 * @param {object|null} options.activity Canonical parsed activity.
 * @param {object|null} options.detection Canonical detected-event model.
 * @param {(activitySecond: number) => void} options.onSetCourseLocation Stores the selected activity-side location.
 * @returns {{containerRef: React.RefObject, style: string, onStyleChange: (style: string) => void, actionPoint: {x: number, y: number, activitySecond: number}|null, onConfirmActionPoint: () => void}}
 */
export default function useVideoSyncMap({ activity, detection, onSetCourseLocation }) {
  const containerRef = useRef(null)
  const controllerRef = useRef(null)
  const [style, setStyle] = useState(VIDEO_SYNC_DEFAULT_MAP_STYLE)
  const [actionPoint, setActionPoint] = useState(null)
  const [error, setError] = useState(null)
  const courseSegments = useMemo(() => buildActivityCourseSegments(activity), [activity])

  useEffect(() => {
    let mounted = true

    async function hydrateStyle() {
      let storedStyle
      try {
        storedStyle = await getPreference(VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY)
      } catch {
        storedStyle = undefined
      }
      if (!mounted || storedStyle === undefined || storedStyle === null) return

      try {
        const nextStyle = requireMapStyle(storedStyle)
        setStyle(nextStyle)
        controllerRef.current?.setStyle(nextStyle)
      } catch (preferenceError) {
        setError(preferenceError)
      }
    }

    void hydrateStyle()
    return () => {
      mounted = false
    }
  }, [])

  useEffect(() => {
    const controller = new VideoSyncMapController(setActionPoint, setError)
    controllerRef.current = controller
    controller.mount(containerRef.current)
    return () => {
      controller.dispose()
      controllerRef.current = null
    }
  }, [])

  useEffect(() => {
    controllerRef.current.setCourseSegments(courseSegments)
  }, [courseSegments])

  useEffect(() => {
    controllerRef.current.setDetection(detection)
  }, [detection])

  const onStyleChange = useCallback((nextStyle) => {
    const validatedStyle = requireMapStyle(nextStyle)
    setStyle(validatedStyle)
    controllerRef.current.setStyle(validatedStyle)
    void setPreference(VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY, validatedStyle).catch(setError)
  }, [])

  const onConfirmActionPoint = useCallback(() => {
    if (actionPoint === null) throw new Error('A course point must be selected before setting the detected location')
    onSetCourseLocation(actionPoint.activitySecond)
    controllerRef.current.clearActionLocation()
  }, [actionPoint, onSetCourseLocation])

  if (error) throw error
  return { containerRef, style, onStyleChange, actionPoint, onConfirmActionPoint }
}
