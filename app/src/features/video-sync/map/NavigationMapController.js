import { LngLat, Marker } from 'maplibre-gl'
import navigationMarkerUrl from '../assets/navigation-marker.svg'
import {
  VIDEO_SYNC_MAP_INITIAL_CENTER,
  VIDEO_SYNC_NAVIGATION_MAP_LOOKAHEAD_METERS,
  VIDEO_SYNC_NAVIGATION_MAP_MAX_PITCH,
  VIDEO_SYNC_NAVIGATION_MAP_PITCH,
  VIDEO_SYNC_NAVIGATION_MAP_TOP_PADDING_RATIO,
  VIDEO_SYNC_NAVIGATION_MAP_ZOOM,
} from '../data/videoSyncConstants'
import { getCourseNavigationCamera } from '../utils/mapPreviewGeometry'
import CourseMapController from './CourseMapController'

export default class NavigationMapController extends CourseMapController {
  constructor() {
    super({
      zoom: VIDEO_SYNC_NAVIGATION_MAP_ZOOM,
      maxZoom: 16.5,
      pitch: VIDEO_SYNC_NAVIGATION_MAP_PITCH,
      maxPitch: VIDEO_SYNC_NAVIGATION_MAP_MAX_PITCH,
      bearing: 0,
      attributionControl: false,
      boxZoom: false,
      dragPan: false,
      dragRotate: false,
      keyboard: false,
      scrollZoom: false,
      touchPitch: false,
      fadeDuration: 0,
      renderWorldCopies: false,
    })
    this.lastBearing = 0
    this.navigationCamera = null
    this.pitch = VIDEO_SYNC_NAVIGATION_MAP_PITCH
    this.previewSecond = null
  }

  onMount() {
    this.map.setTransformCameraUpdate(() =>
      this.navigationCamera === null ? {} : { center: this.navigationCamera.center, bearing: this.navigationCamera.bearing },
    )
    this.map.scrollZoom.enable({ around: 'center' })
    this.map.touchZoomRotate.disableRotation()
    const markerElement = document.createElement('img')
    markerElement.src = navigationMarkerUrl
    markerElement.alt = ''
    markerElement.className = 'video-sync-navigation-marker pointer-events-none size-10 drop-shadow-[0_2px_4px_rgba(0,0,0,0.85)]'
    markerElement.style.display = 'none'
    this.navigationMarker = new Marker({ element: markerElement, anchor: 'center', subpixelPositioning: true })
      .setLngLat(VIDEO_SYNC_MAP_INITIAL_CENTER)
      .addTo(this.map)
  }

  onDispose() {
    this.navigationMarker.remove()
  }

  onCourseChange() {
    this.syncCamera()
  }

  setPreviewSecond(previewSecond) {
    this.previewSecond = previewSecond
    this.syncCamera()
  }

  setPitch(pitch) {
    this.pitch = pitch
    this.map.jumpTo({ pitch })
  }

  syncCamera() {
    if (this.previewSecond === null) {
      this.navigationCamera = null
      this.hideMarker()
      return
    }
    const camera = getCourseNavigationCamera(this.courseSegments, this.previewSecond, VIDEO_SYNC_NAVIGATION_MAP_LOOKAHEAD_METERS)
    if (camera === null) {
      this.navigationCamera = null
      this.hideMarker()
      return
    }

    if (camera.bearing !== null) this.lastBearing = camera.bearing
    this.navigationCamera = { center: LngLat.convert(camera.center), bearing: this.lastBearing }
    this.navigationMarker.setLngLat(camera.center)
    this.navigationMarker.getElement().style.display = ''
    if (this.map.isZooming()) return
    this.map.jumpTo({
      center: camera.center,
      bearing: this.lastBearing,
      pitch: this.pitch,
      padding: {
        top: this.map.getContainer().clientHeight * VIDEO_SYNC_NAVIGATION_MAP_TOP_PADDING_RATIO,
        right: 0,
        bottom: 0,
        left: 0,
      },
    })
  }

  hideMarker() {
    this.navigationMarker.getElement().style.display = 'none'
  }

  onResize() {
    this.syncCamera()
  }
}
