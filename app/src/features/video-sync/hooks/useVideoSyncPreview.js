import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { getMapStyleUrlTemplate, listMapStyles } from '@/api/backend'
import { getPreference, setPreference } from '@/lib/preferences-store'
import { useWasmWidgets } from '@/features/wasm-widgets/hooks/useWasmWidgets'
import {
  VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY,
  VIDEO_SYNC_NAVIGATION_MAP_PITCH,
  VIDEO_SYNC_SPEED_FONT,
  VIDEO_SYNC_SPEED_WIDGETS,
  VIDEO_SYNC_SPEED_SCENE,
  VIDEO_SYNC_DISABLED_LAPS,
  VIDEO_SYNC_EMPTY_WIDGETS,
} from '../data/videoSyncConstants'
import { buildActivityCourseSegments } from '../utils/activitySyncInput'
import { buildVideoSyncScreenLayout } from '../utils/videoSyncPresentation'
import NavigationMapController from '../map/NavigationMapController'
import SelectionMapController from '../map/SelectionMapController'

/** A style that has since been uninstalled is still a style id: the map then draws the first installed one. */
function requireMapStylePreference(value) {
  if (typeof value !== 'string') {
    throw new Error(`Preference "${VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY}" must be a map style id`)
  }
  return value
}

/**
 * Owns the map pair and the speed readout for the video-sync preview.
 *
 * @param {object} options Preview inputs and course-location actions.
 * @returns {object} Map container refs and presentation actions.
 */
export default function useVideoSyncPreview({
  activity,
  detection,
  displayScale,
  onSetCourseLocation,
  onDeleteCourseLocation,
  previewSecond,
  sceneSize,
}) {
  const selectionMapRef = useRef(null)
  const navigationMapRef = useRef(null)
  const selectionControllerRef = useRef(null)
  const navigationControllerRef = useRef(null)
  const actionsRef = useRef({ onSetCourseLocation, onDeleteCourseLocation })
  const [actionPoint, setActionPoint] = useState(null)
  const [pitch, setPitch] = useState(VIDEO_SYNC_NAVIGATION_MAP_PITCH)
  const [styles, setStyles] = useState([])
  // The chosen style id, or null to draw the first installed style.
  const [chosenStyle, setChosenStyle] = useState(null)
  const [styleUrlTemplate, setStyleUrlTemplate] = useState(null)
  const [error, setError] = useState(null)
  const screenLayout = buildVideoSyncScreenLayout(sceneSize, displayScale)
  const courseSegments = useMemo(() => buildActivityCourseSegments(activity), [activity])
  const hasSpeed = useMemo(() => activity?.speed?.some((value) => value !== null && value !== undefined) ?? false, [activity])
  const speedScene = useMemo(
    () => ({ ...VIDEO_SYNC_SPEED_SCENE, width: sceneSize.width, height: sceneSize.height, scale: displayScale }),
    [sceneSize.width, sceneSize.height, displayScale],
  )
  const { attach, error: speedError } = useWasmWidgets({
    widgets: hasSpeed ? VIDEO_SYNC_SPEED_WIDGETS : VIDEO_SYNC_EMPTY_WIDGETS,
    scene: speedScene,
    activity,
    disabledLaps: VIDEO_SYNC_DISABLED_LAPS,
    second: previewSecond,
    defaultFont: VIDEO_SYNC_SPEED_FONT,
  })
  const speedCanvasRef = useCallback((canvas) => attach(VIDEO_SYNC_SPEED_WIDGETS[0].id, canvas), [attach])
  // A style that isn't installed draws the first installed one, as the backend resolves it.
  const style = (styles.find((installed) => installed.id === chosenStyle) ?? styles[0])?.id ?? null
  const styleUrl = styleUrlTemplate === null || style === null ? null : styleUrlTemplate.replace('{style}', style)

  useEffect(() => {
    let mounted = true
    Promise.all([getMapStyleUrlTemplate(), listMapStyles(), getPreference(VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY).catch(() => undefined)])
      .then(([template, installedStyles, storedStyle]) => {
        if (!mounted) return
        setStyleUrlTemplate(template)
        setStyles(installedStyles)
        if (storedStyle !== undefined && storedStyle !== null) setChosenStyle(requireMapStylePreference(storedStyle))
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
    setChosenStyle(nextStyle)
    void setPreference(VIDEO_SYNC_MAP_STYLE_PREFERENCE_KEY, nextStyle).catch(setError)
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
  return {
    selectionMapRef,
    navigationMapRef,
    actionPoint,
    onConfirmActionPoint,
    pitch,
    onPitchChange,
    screenLayout,
    hasSpeed,
    speedCanvasRef,
    speedError,
    style,
    styles,
    onStyleChange,
  }
}
