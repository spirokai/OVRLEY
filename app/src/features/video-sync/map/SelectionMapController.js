import { LngLatBounds, Marker, Popup } from 'maplibre-gl'
import { VIDEO_SYNC_MAP_INITIAL_ZOOM } from '../data/videoSyncConstants'
import { getCoursePositionAtActivitySecond, getSnappedCoursePosition } from '../utils/mapPreviewGeometry'
import CourseMapController from './CourseMapController'

export default class SelectionMapController extends CourseMapController {
  constructor(onActionPointChange, onSetCourseLocation, onDeleteCourseLocation) {
    super({ zoom: VIDEO_SYNC_MAP_INITIAL_ZOOM, pitch: 0, maxPitch: 0, touchPitch: false, attributionControl: true })
    this.onActionPointChange = onActionPointChange
    this.onSetCourseLocation = onSetCourseLocation
    this.onDeleteCourseLocation = onDeleteCourseLocation
    this.canvasContainer = null
    this.detection = null
    this.detectedLocationMarker = null
    this.playbackMarker = null
    this.previewSecond = null
    this.hoverMarker = null
    this.actionLocation = null
    this.hasFittedCourse = false

    this.handleClick = this.handleClick.bind(this)
    this.handleMouseMove = this.handleMouseMove.bind(this)
    this.handleMouseOut = this.handleMouseOut.bind(this)
    this.handleMove = this.handleMove.bind(this)
    this.handleDetectedLocationDragEnd = this.handleDetectedLocationDragEnd.bind(this)
    this.handleDeleteDetectedLocation = this.handleDeleteDetectedLocation.bind(this)
  }

  onMount() {
    this.canvasContainer = this.map.getCanvasContainer()
    this.map.on('click', this.handleClick)
    this.map.on('mousemove', this.handleMouseMove)
    this.map.on('mouseout', this.handleMouseOut)
    this.map.on('move', this.handleMove)
  }

  onDispose() {
    this.syncHoverTarget(null)
    this.removeDetectedLocationMarker()
    this.removePlaybackMarker()
    this.canvasContainer = null
  }

  onCourseChange() {
    this.syncDetectedLocationMarker()
    this.syncPlaybackMarker()
  }

  setDetection(detection) {
    this.detection = detection
    this.syncDetectedLocationMarker()
  }

  setPreviewSecond(previewSecond) {
    this.previewSecond = previewSecond
    this.syncPlaybackMarker()
  }

  onCourseRendered() {
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
      this.removeDetectedLocationMarker()
      return
    }

    if (this.detectedLocationMarker === null) {
      const popupContent = document.createElement('button')
      popupContent.type = 'button'
      popupContent.className =
        'flex size-5 items-center justify-center rounded-full border-2 border-red-400 bg-white text-sm leading-none font-bold text-red-600 shadow-sm hover:bg-red-100'
      popupContent.setAttribute('aria-label', 'Delete location marker')
      popupContent.textContent = '×'
      popupContent.addEventListener('click', this.handleDeleteDetectedLocation)

      const popup = new Popup({ anchor: 'bottom-left', className: 'video-sync-location-popup', closeButton: false, offset: [6, -20] }).setDOMContent(
        popupContent,
      )
      this.detectedLocationMarker = new Marker({ color: 'var(--color-video-sync-location)', scale: 1, draggable: true })
        .setLngLat(coordinate)
        .setPopup(popup)
        .addTo(this.map)
      this.detectedLocationMarker.on('dragend', this.handleDetectedLocationDragEnd)
      return
    }

    this.detectedLocationMarker.setLngLat(coordinate)
  }

  removeDetectedLocationMarker() {
    if (this.detectedLocationMarker === null) return
    this.detectedLocationMarker.off('dragend', this.handleDetectedLocationDragEnd)
    this.detectedLocationMarker.remove()
    this.detectedLocationMarker = null
  }

  syncPlaybackMarker() {
    const coordinate = this.previewSecond === null ? null : getCoursePositionAtActivitySecond(this.courseSegments, this.previewSecond)
    if (coordinate === null) {
      this.removePlaybackMarker()
      return
    }

    if (this.playbackMarker === null) {
      const element = document.createElement('div')
      element.className = 'pointer-events-none size-4 rounded-full border-1 border-white bg-[#EF6C15] shadow-[0_0_0_8px_rgba(239,108,21,0.35)]'
      this.playbackMarker = new Marker({ element }).setLngLat(coordinate).addTo(this.map)
      return
    }

    this.playbackMarker.setLngLat(coordinate)
  }

  removePlaybackMarker() {
    this.playbackMarker?.remove()
    this.playbackMarker = null
  }

  handleDetectedLocationDragEnd() {
    const marker = this.detectedLocationMarker
    const location = marker === null ? null : getSnappedCoursePosition(this.map, this.map.project(marker.getLngLat()), this.courseSegments)
    if (location === null) {
      const currentCoordinate =
        this.detection === null || this.detection.location === null
          ? null
          : getCoursePositionAtActivitySecond(this.courseSegments, this.detection.location.time)
      if (currentCoordinate !== null) marker?.setLngLat(currentCoordinate)
      return
    }
    marker.setLngLat(location.position)
    this.onSetCourseLocation(location.activitySecond)
  }

  handleDeleteDetectedLocation(event) {
    event.stopPropagation()
    this.onDeleteCourseLocation()
  }

  handleClick(event) {
    const clickedElement = event.originalEvent?.target
    const markerElement = this.detectedLocationMarker?.getElement()
    if (clickedElement && markerElement?.contains(clickedElement)) return

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
