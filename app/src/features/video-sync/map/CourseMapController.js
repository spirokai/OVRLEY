import { Map, NavigationControl } from 'maplibre-gl'
import { VIDEO_SYNC_MAP_INITIAL_CENTER, VIDEO_SYNC_MAP_RESIZE_SETTLE_DELAY_MS } from '../data/videoSyncConstants'
import { createCourseGeoJson } from '../utils/mapPreviewGeometry'

const COURSE_SOURCE_ID = 'activity-course'
const EMPTY_STYLE = { version: 8, sources: {}, layers: [] }

// Both previews own separate maps, but share the route and MapLibre lifecycle.
export default class CourseMapController {
  constructor(options) {
    this.options = options
    this.courseSegments = []
    this.map = null
    this.styleUrl = null
    this.styleLoaded = false
    this.styleLoadPending = false
    this.resizeTimeout = null
    this.handleStyleLoad = this.handleStyleLoad.bind(this)
    this.handleResize = this.handleResize.bind(this)
  }

  mount(container) {
    this.map = new Map({ container, style: EMPTY_STYLE, center: VIDEO_SYNC_MAP_INITIAL_CENTER, ...this.options })
    this.map.addControl(new NavigationControl({ showCompass: false }), 'top-right')
    this.map.on('style.load', this.handleStyleLoad)
    this.onMount()
    this.resizeObserver = new ResizeObserver(this.handleResize)
    this.resizeObserver.observe(container)
  }

  dispose() {
    this.resizeObserver.disconnect()
    if (this.resizeTimeout !== null) window.clearTimeout(this.resizeTimeout)
    this.onDispose()
    this.map.remove()
    this.map = null
  }

  setStyleUrl(styleUrl) {
    if (styleUrl === null || styleUrl === this.styleUrl) return
    this.styleUrl = styleUrl
    this.styleLoaded = false
    this.styleLoadPending = true
    this.map.setStyle(styleUrl)
  }

  setCourseSegments(courseSegments) {
    this.courseSegments = courseSegments
    if (this.styleLoaded) this.syncCourse()
    this.onCourseChange()
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
        id: 'activity-course-line',
        type: 'line',
        source: COURSE_SOURCE_ID,
        layout: { 'line-cap': 'round', 'line-join': 'round' },
        paint: { 'line-color': '#EF6C15', 'line-opacity': 1, 'line-width': 6 },
      })
    }
    this.onCourseRendered()
  }

  handleResize() {
    if (this.resizeTimeout !== null) window.clearTimeout(this.resizeTimeout)
    this.resizeTimeout = window.setTimeout(() => {
      this.resizeTimeout = null
      this.map.resize()
      this.onResize()
    }, VIDEO_SYNC_MAP_RESIZE_SETTLE_DELAY_MS)
  }

  onMount() {}
  onDispose() {}
  onCourseChange() {}
  onCourseRendered() {}
  onResize() {}
}
