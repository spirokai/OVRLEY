import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { getMapStyleUrlTemplate } from '@/api/backend'
import { getPreference, setPreference } from '@/lib/preferences-store'
import {
  VIDEO_SYNC_DEFAULT_MAP_STYLE,
  VIDEO_SYNC_MAP_STYLES,
  VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY,
  VIDEO_SYNC_NAVIGATION_MAP_PITCH,
} from '../data/videoSyncConstants'
import { buildActivityCourseSegments } from '../utils/activitySyncInput'
import NavigationMapController from '../map/NavigationMapController'
import SelectionMapController from '../map/SelectionMapController'

function requireMapStyle(value) {
  if (!VIDEO_SYNC_MAP_STYLES.includes(value)) {
    throw new Error(`Preference "${VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY}" must be a supported map style`)
  }
  return value
}

/**
 * Owns both video-sync maps, their shared course, and the selected map style.
 *
 * @param {object} options Preview inputs and course-location actions.
 * @returns {object} Map container refs and presentation actions.
 */
export default function useVideoSyncMaps({ activity, detection, onSetCourseLocation, onDeleteCourseLocation, previewSecond }) {
  const selectionMapRef = useRef(null)
  const navigationMapRef = useRef(null)
  const selectionControllerRef = useRef(null)
  const navigationControllerRef = useRef(null)
  const actionsRef = useRef({ onSetCourseLocation, onDeleteCourseLocation })
  const [actionPoint, setActionPoint] = useState(null)
  const [pitch, setPitch] = useState(VIDEO_SYNC_NAVIGATION_MAP_PITCH)
  const [style, setStyle] = useState(VIDEO_SYNC_DEFAULT_MAP_STYLE)
  const [styleUrlTemplate, setStyleUrlTemplate] = useState(null)
  const [error, setError] = useState(null)
  const courseSegments = useMemo(() => buildActivityCourseSegments(activity), [activity])
  const styleUrl = styleUrlTemplate === null ? null : styleUrlTemplate.replace('{style}', style)

  useEffect(() => {
    let mounted = true
    Promise.all([getMapStyleUrlTemplate(), getPreference(VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY).catch(() => undefined)])
      .then(([template, storedStyle]) => {
        if (!mounted) return
        setStyleUrlTemplate(template)
        if (storedStyle !== undefined && storedStyle !== null) setStyle(requireMapStyle(storedStyle))
      })
      .catch((loadError) => {
        if (mounted) setError(loadError)
      })
    return () => {
      mounted = false
    }
  }, [])

  useEffect(() => {
    actionsRef.current = { onSetCourseLocation, onDeleteCourseLocation }
  }, [onSetCourseLocation, onDeleteCourseLocation])

  useEffect(() => {
    const selection = new SelectionMapController(
      setActionPoint,
      (activitySecond) => actionsRef.current.onSetCourseLocation(activitySecond),
      () => actionsRef.current.onDeleteCourseLocation(),
    )
    const navigation = new NavigationMapController()
    selectionControllerRef.current = selection
    navigationControllerRef.current = navigation
    selection.mount(selectionMapRef.current)
    navigation.mount(navigationMapRef.current)
    return () => {
      selection.dispose()
      navigation.dispose()
      selectionControllerRef.current = null
      navigationControllerRef.current = null
    }
  }, [])

  useEffect(() => {
    selectionControllerRef.current.setStyleUrl(styleUrl)
    navigationControllerRef.current.setStyleUrl(styleUrl)
  }, [styleUrl])

  useEffect(() => {
    selectionControllerRef.current.setCourseSegments(courseSegments)
    navigationControllerRef.current.setCourseSegments(courseSegments)
  }, [courseSegments])

  useEffect(() => {
    selectionControllerRef.current.setPreviewSecond(previewSecond)
    navigationControllerRef.current.setPreviewSecond(previewSecond)
  }, [previewSecond])

  useEffect(() => {
    selectionControllerRef.current.setDetection(detection)
  }, [detection])

  const onStyleChange = useCallback((nextStyle) => {
    const validatedStyle = requireMapStyle(nextStyle)
    setStyle(validatedStyle)
    void setPreference(VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY, validatedStyle).catch(setError)
  }, [])

  const onPitchChange = useCallback((nextPitch) => {
    setPitch(nextPitch)
    navigationControllerRef.current.setPitch(nextPitch)
  }, [])

  const onConfirmActionPoint = useCallback(() => {
    if (actionPoint === null) throw new Error('A course point must be selected before setting the detected location')
    onSetCourseLocation(actionPoint.activitySecond)
    selectionControllerRef.current.clearActionLocation()
  }, [actionPoint, onSetCourseLocation])

  if (error) throw error
  return { selectionMapRef, navigationMapRef, actionPoint, onConfirmActionPoint, pitch, onPitchChange, style, onStyleChange }
}
