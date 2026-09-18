/**
 * Owns manual video-sync timeline graph geometry and landmark drag state.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { secondsToViewPx } from '@/features/player/utils/timelineGeometry'
import { VIDEO_SYNC_LANDMARK_PRESENTATION } from '../data/videoSyncConstants'
import { buildEventBands, buildGraphGeometry, buildGraphScales } from '../utils/graphGeometry'
import useVideoSyncLandmarkDrag from './useVideoSyncLandmarkDrag'

const EMPTY_GRAPH_SERIES = Object.freeze({ speed: [], turning: [] })
const EMPTY_GRAPH_GEOMETRY = Object.freeze({
  heightPx: 0,
  paths: Object.freeze({ speed: '', turning: '' }),
  scales: null,
  widthPx: 0,
})

/**
 * Creates the timeline graph and landmark presentation model for sync mode.
 *
 * @param {object} options Timeline and manual-sync inputs.
 * @param {boolean} [options.enabled=false] Whether sync timeline presentation is active.
 * @param {object|null} options.containerElement Measured timeline element.
 * @param {number} options.viewStart Current timeline viewport start.
 * @param {number} options.viewEnd Current timeline viewport end.
 * @param {number} options.widthPx Measured timeline width.
 * @param {number} options.timelineMinimum Minimum legal timeline second.
 * @param {number} options.totalDuration Maximum legal timeline second.
 * @param {number|null} options.videoDuration Imported video duration.
 * @param {number} options.videoSyncOffsetSeconds Committed video offset.
 * @param {number|null} options.videoSyncOffsetPreviewSeconds Display-only video offset preview.
 * @param {object|null} options.detection Detector result containing graph series and events.
 * @param {object[]} options.landmarks Canonical video landmarks.
 * @param {function} options.scrubTo Timeline scrub callback for in-view handles.
 * @param {function} options.moveLandmark Store action for committing a landmark position.
 * @param {function} [options.followSecond] Existing viewport edge-follow callback.
 * @returns {{graph: object|null, landmarks: object[]}} Render-ready sync timeline models.
 */
export default function useVideoSyncTimeline({
  enabled = false,
  containerElement,
  viewStart,
  viewEnd,
  widthPx,
  timelineMinimum,
  totalDuration,
  videoDuration,
  videoSyncOffsetSeconds,
  videoSyncOffsetPreviewSeconds,
  detection,
  landmarks,
  scrubTo,
  moveLandmark,
  followSecond,
}) {
  const graphFrameRef = useRef(null)
  const graphInputsRef = useRef(null)

  const graphSeries = detection?.graphSeries ?? EMPTY_GRAPH_SERIES
  const graphScales = useMemo(() => buildGraphScales(graphSeries), [graphSeries])
  const [graphGeometry, setGraphGeometry] = useState(() =>
    enabled ? buildGraphGeometry({ graphSeries, scales: graphScales, viewStart, viewEnd, widthPx }) : EMPTY_GRAPH_GEOMETRY,
  )

  useEffect(() => {
    graphInputsRef.current = { graphSeries, scales: graphScales, viewStart, viewEnd, widthPx }
  }, [graphScales, graphSeries, viewEnd, viewStart, widthPx])

  const replaceGraphGeometry = useCallback(() => {
    if (graphFrameRef.current !== null) return

    const update = () => {
      graphFrameRef.current = null
      const inputs = graphInputsRef.current
      if (!inputs) return
      setGraphGeometry(buildGraphGeometry(inputs))
    }

    if (typeof window === 'undefined' || typeof window.requestAnimationFrame !== 'function') {
      queueMicrotask(update)
      return
    }
    graphFrameRef.current = window.requestAnimationFrame(update)
  }, [])

  useEffect(() => {
    if (!enabled) {
      if (graphFrameRef.current !== null) window.cancelAnimationFrame?.(graphFrameRef.current)
      graphFrameRef.current = null
      setGraphGeometry(EMPTY_GRAPH_GEOMETRY)
      return undefined
    }

    replaceGraphGeometry()
    return undefined
  }, [enabled, graphSeries, graphScales, replaceGraphGeometry, viewEnd, viewStart, widthPx])

  useEffect(
    () => () => {
      if (graphFrameRef.current !== null) window.cancelAnimationFrame?.(graphFrameRef.current)
    },
    [],
  )

  const displayedVideoOffset = videoSyncOffsetPreviewSeconds ?? videoSyncOffsetSeconds

  const { dragPreview, getLandmarkPointerProps } = useVideoSyncLandmarkDrag({
    containerElement,
    enabled,
    followSecond,
    moveLandmark,
    scrubTo,
    timelineMinimum,
    totalDuration,
    videoDuration,
    videoSyncOffsetSeconds,
    viewEnd,
    viewStart,
    widthPx,
  })

  const landmarkModels = useMemo(() => {
    if (!enabled || videoDuration === null || widthPx <= 0 || viewEnd <= viewStart) return []

    return landmarks.map((landmark) => {
      const presentation = VIDEO_SYNC_LANDMARK_PRESENTATION[landmark.type]
      const previewSecond = dragPreview?.id === landmark.id ? dragPreview.videoSecond : landmark.videoSecond
      const timelineSecond = displayedVideoOffset + previewSecond
      const isInView = timelineSecond >= viewStart && timelineSecond <= viewEnd
      const isActiveDrag = dragPreview?.id === landmark.id
      const isClamped = !isInView
      const leftSecond = isInView || timelineSecond > viewEnd ? viewEnd : viewStart
      const left = isClamped ? 0 : secondsToViewPx({ second: timelineSecond, viewStart, viewEnd, widthPx })
      const clampedLeft = isClamped ? (leftSecond === viewEnd ? widthPx : 0) : left

      return {
        Icon: presentation.Icon,
        handleProps: isInView || isActiveDrag ? getLandmarkPointerProps({ ...landmark, timelineSecond }) : null,
        handleStyle: { left: clampedLeft },
        id: landmark.id,
        isClamped,
        isInteractive: isInView || isActiveDrag,
        defaultLabel: presentation.defaultLabel,
        labelKey: presentation.labelKey,
        lineClassName: presentation.stripe,
        lineStyle: { left: clampedLeft },
        iconClassName: presentation.className,
        timelineSecond,
        type: landmark.type,
        videoSecond: previewSecond,
      }
    })
  }, [displayedVideoOffset, dragPreview, enabled, getLandmarkPointerProps, landmarks, videoDuration, viewEnd, viewStart, widthPx])

  const eventBands = useMemo(() => buildEventBands({ detection, viewEnd, viewStart, widthPx }), [detection, viewEnd, viewStart, widthPx])

  if (!enabled) return { graph: null, landmarks: [] }

  return {
    graph: {
      ...graphGeometry,
      eventBands,
    },
    landmarks: landmarkModels,
  }
}
