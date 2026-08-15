/**
 * Top-level player orchestration hook for the presentational overlay player components.
 */

import { useEffect, useEffectEvent, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { hasOpenOverlay, isFormFieldShortcut, matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { videoOverlapsActivity } from '@/lib/video-timing'
import useStore from '@/store/useStore'
import { formatTimelineTime, snapTimelineSecondToFrame } from '../utils/playerTiming'
import { moveClipOffset, roundToDevicePixel, secondsToViewPx } from '../utils/timelineGeometry'

import useClipDrag from './useClipDrag'
import useExportRangeTimeline from './useExportRangeTimeline'
import usePlaybackEngine from './usePlaybackEngine'
import useTimelineClips from './useTimelineClips'
import useTimelineGestures from './useTimelineGestures'
import useTimelineViewport from './useTimelineViewport'

function getDevicePixelRatio() {
  if (typeof window === 'undefined') return 1
  return window.devicePixelRatio || 1
}

function getMarkerClassName(marker) {
  return `mt-1 h-4 w-2.5 bg-success/10 ${
    marker === 'from' ? 'rounded-l-sm border-b-2 border-l-2 border-t-2' : 'rounded-r-sm border-b-2 border-r-2 border-t-2'
  } border-success`
}

/**
 * Composes store selection, playback, viewport, export range, gestures, clips, and keyboard commands.
 *
 * @param {{ activeKeyboardWorkspace: string, backgroundMode: string }} options Overlay player inputs.
 * @param {string} options.activeKeyboardWorkspace Active keyboard command workspace.
 * @param {string} options.backgroundMode Active preview background mode.
 * @returns {object} Presentational view model for the overlay player.
 */
export default function useOverlayPlayer({ activeKeyboardWorkspace, backgroundMode }) {
  // Store selector - gathers the entire player-facing store contract in one subscription.
  const playerStore = useStore(
    useShallow((state) => ({
      activityFilename: state.activityFilename,
      activitySummary: state.activitySummary,
      beginPreviewScrub: state.beginPreviewScrub,
      commitPreviewScrub: state.commitPreviewScrub,
      fallbackDurationSeconds: state.fallbackDurationSeconds,
      importedVideoDuration: state.importedVideoDuration,
      importedVideoFps: state.importedVideoFps,
      importedVideoImportId: state.importedVideoImportId,
      importedVideoPath: state.importedVideoPath,
      pausePreviewPlayback: state.pausePreviewPlayback,
      toggleVideoMute: state.toggleVideoMute,
      isVideoMuted: state.isVideoMuted,
      previewPlaybackSource: state.previewPlaybackSource,
      previewPlaybackState: state.previewPlaybackState,
      sceneFps: state.config?.scene?.fps ?? 30,
      selectedSecond: state.selectedSecond,
      setSelectedSecond: state.setSelectedSecond,
      setVideoSyncOffset: state.setVideoSyncOffset,
      setVideoSyncOffsetPreview: state.setVideoSyncOffsetPreview,
      setVideoSyncWarning: state.setVideoSyncWarning,
      startPreviewPlayback: state.startPreviewPlayback,
      selectedWidgetIds: state.selectedWidgetIds,
      updatePreviewScrub: state.updatePreviewScrub,
      updateRate: state.updateRate,
      videoSyncOffsetSeconds: state.videoSyncOffsetSeconds,
      videoSyncOffsetPreviewSeconds: state.videoSyncOffsetPreviewSeconds,
    })),
  )

  const [selectedClipId, setSelectedClipId] = useState(null)
  const [keyboardSyncOffsetSeconds, setKeyboardSyncOffsetSeconds] = useState(null)

  // Durable domains - each hook owns behavior for one player concern, while this hook wires them together.
  const playback = usePlaybackEngine({ ...playerStore, backgroundMode })
  const { hasActivity, isPlaying, pause, play, stepBySeconds } = playback
  const exportBoundarySecond = playback.importedVideoPath
    ? snapTimelineSecondToFrame(playback.clampedPlayhead, playerStore.importedVideoFps, playback.videoSyncOffsetSeconds)
    : playback.clampedPlayhead
  const exportTimeline = useExportRangeTimeline({
    defaultEndSecond: playback.importedVideoPath ? playback.videoSyncOffsetSeconds + playback.importedVideoDuration : playback.totalDuration,
    timelineMinimum: playback.timelineMinimum,
    totalDuration: playback.totalDuration,
  })
  const gestures = useTimelineGestures({
    cancelMarkerPreview: exportTimeline.cancelMarkerPreview,
    cancelScrub: playback.cancelScrub,
    commitMarker: exportTimeline.commitMarker,
    commitScrub: playback.commitScrub,
    previewMarker: exportTimeline.previewMarker,
    scrubTo: playback.scrubTo,
  })
  const { getExportMarkerProps, updateTimelineMetrics } = gestures

  // Media flags - downstream hooks need explicit availability booleans, not inferred path/summary checks.
  const hasVideo = Boolean(playback.importedVideoPath)
  const hasActivityData = Boolean(playerStore.activitySummary)
  const activityDurationSeconds = playerStore.activitySummary?.durationSeconds ?? 0
  const canSelectClip = hasVideo && playback.hasActivity

  const nudgeClip = (laneId, deltaSeconds) => {
    setKeyboardSyncOffsetSeconds((currentOffset) => {
      const nextOffset = moveClipOffset({
        currentOffset: currentOffset ?? playerStore.videoSyncOffsetSeconds,
        deltaSeconds,
        laneId,
      })
      const roundedOffset = Math.round(nextOffset * 10) / 10
      return videoOverlapsActivity({ videoStart: roundedOffset, videoDuration: playback.importedVideoDuration })
        ? roundedOffset
        : (currentOffset ?? playerStore.videoSyncOffsetSeconds)
    })
  }

  const commitClipNudge = () => {
    if (keyboardSyncOffsetSeconds === null) return
    playerStore.setVideoSyncOffset(keyboardSyncOffsetSeconds)
    setKeyboardSyncOffsetSeconds(null)
  }

  // Selection is scoped to the current media pair and ends when the user clicks elsewhere.
  useEffect(() => {
    setSelectedClipId(null)
    setKeyboardSyncOffsetSeconds(null)
  }, [playerStore.activitySummary, playerStore.importedVideoImportId, playback.importedVideoPath])

  useEffect(() => {
    const clearClipSelection = () => setSelectedClipId(null)
    window.addEventListener('pointerdown', clearClipSelection, true)
    return () => window.removeEventListener('pointerdown', clearClipSelection, true)
  }, [])

  // Clip drag - owns horizontal drag gesture state for sync offset adjustment.
  const clipDrag = useClipDrag({
    setVideoSyncOffset: playerStore.setVideoSyncOffset,
    setVideoSyncOffsetPreview: playerStore.setVideoSyncOffsetPreview,
    activityDurationSeconds,
    importedVideoDuration: playback.importedVideoDuration,
  })
  const { getLaneDragProps, isDragging: isClipDragging, snapGuidelineSecond, updateMetrics: updateClipDragMetrics } = clipDrag
  const timelineVideoSyncOffsetSeconds = keyboardSyncOffsetSeconds ?? playerStore.videoSyncOffsetPreviewSeconds ?? playback.videoSyncOffsetSeconds

  // Viewport domain - owns measurement, fit targets, ticks, zoom, pan, and playback follow behavior.
  const viewport = useTimelineViewport({
    activityDurationSeconds,
    fallbackDurationSeconds: playerStore.fallbackDurationSeconds,
    hasActivityData,
    hasVideo,
    importedVideoDuration: playback.importedVideoDuration,
    isDragging: gestures.isTimelineDragging || isClipDragging,
    isPlaying: playback.isPlaying,
    playheadSecond: playback.clampedPlayhead,
    totalDuration: playback.totalDuration,
    videoSyncOffsetPreviewSeconds: timelineVideoSyncOffsetSeconds,
    videoSyncOffsetSeconds: playback.videoSyncOffsetSeconds,
  })
  const { displayedFitTargetId, fitTarget, fitTargets: viewportFitTargets, viewport: timelineViewport, widthPx } = viewport

  // Gesture metrics sync - pointer math uses the latest measured element and viewport without re-rendering on every move.
  useEffect(() => {
    const metrics = {
      containerElement: viewport.containerElement,
      followSecond: viewport.followSecond,
      panBy: viewport.panBy,
      timelineMinimum: viewport.timelineMinimum,
      totalDuration: playback.totalDuration,
      viewEnd: viewport.viewport.viewEnd,
      viewStart: viewport.viewport.viewStart,
      widthPx: viewport.widthPx,
    }
    updateTimelineMetrics(metrics)
    updateClipDragMetrics({
      ...metrics,
      hasVideo,
      videoSyncOffsetSeconds: playback.videoSyncOffsetSeconds,
      activityDurationSeconds,
      importedVideoDuration: playback.importedVideoDuration,
    })
  }, [
    activityDurationSeconds,
    hasVideo,
    playback.importedVideoDuration,
    playback.totalDuration,
    playback.videoSyncOffsetSeconds,
    viewport.containerElement,
    viewport.containerRef,
    viewport.followSecond,
    viewport.timelineMinimum,
    viewport.panBy,
    viewport.viewport.viewEnd,
    viewport.viewport.viewStart,
    viewport.widthPx,
    updateClipDragMetrics,
    updateTimelineMetrics,
  ])

  // Keyboard shortcuts - global commands route through the same APIs as toolbar buttons.
  const onKeyDown = useEffectEvent((event) => {
    if (event.repeat || event.defaultPrevented || hasOpenOverlay()) return

    const match = matchKeyboardShortcut(event, 'player')
    if (!match || isFormFieldShortcut(event)) return

    switch (match.commandId) {
      case 'player.stepBackward':
      case 'player.stepForward':
        if (activeKeyboardWorkspace !== 'player' || !hasActivity) return
        event.preventDefault()
        stepBySeconds(match.commandId === 'player.stepForward' ? 1 : -1)
        return
      case 'player.togglePlayback':
        if (!hasActivity) return
        event.preventDefault()
        if (isPlaying) pause()
        else play()
        return
      case 'player.resetToStart':
        if (!hasActivity) return
        event.preventDefault()
        playback.resetToStart()
        return
      case 'player.jumpToEnd':
        if (!hasActivity) return
        event.preventDefault()
        playback.jumpToEnd()
        return
      case 'player.toggleMute':
        if (!hasVideo) return
        event.preventDefault()
        playerStore.toggleVideoMute()
        return
      case 'export.setIn':
        if (!hasActivityData && !hasVideo) return
        event.preventDefault()
        exportTimeline.setBoundary('from', exportBoundarySecond)
        return
      case 'export.setOut':
        if (!hasActivityData && !hasVideo) return
        event.preventDefault()
        exportTimeline.setBoundary('to', exportBoundarySecond)
        return
      case 'export.clear':
        if (!hasActivityData && !hasVideo) return
        event.preventDefault()
        exportTimeline.clear()
        return
      case 'export.goToIn':
        if (!exportTimeline.isCustom) return
        event.preventDefault()
        playback.commitScrub(exportTimeline.fromSecond)
        return
      case 'export.goToOut':
        if (!exportTimeline.isCustom) return
        event.preventDefault()
        playback.commitScrub(exportTimeline.toSecond)
        return
      case 'timeline.fitAll':
        if (!playback.hasActivity && !hasVideo) return
        event.preventDefault()
        viewport.resetView()
        return
      case 'timeline.fitMedia':
        if (!viewport.fitTargets.some((target) => target.id === 'all')) return
        event.preventDefault()
        viewport.fitTarget('all')
        return
      case 'timeline.fitActivity':
        if (!viewport.fitTargets.some((target) => target.id === 'activity')) return
        event.preventDefault()
        viewport.fitTarget('activity')
        return
      case 'timeline.fitVideo':
        if (!viewport.fitTargets.some((target) => target.id === 'video')) return
        event.preventDefault()
        viewport.fitTarget('video')
        return
      case 'sync.globalNudge':
        if (!hasVideo || !hasActivityData) return
        event.preventDefault()
        try {
          playerStore.setVideoSyncOffset(playerStore.videoSyncOffsetSeconds + match.binding.seconds)
          playerStore.setVideoSyncWarning(null)
        } catch (error) {
          playerStore.setVideoSyncWarning(error.message)
        }
        return
      default:
        return
    }
  })

  useEffect(() => {
    if (typeof window === 'undefined') return undefined

    const handleKeyDown = (event) => onKeyDown(event)
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  // Lane models - clip geometry and tooltip state are prepared before presentational rendering.
  const lanes = useTimelineClips({
    activityFilename: playerStore.activityFilename,
    activitySummary: playerStore.activitySummary,
    canSelect: canSelectClip,
    commitClipNudge,
    exportHighlightRange: exportTimeline.highlightRange,
    getLaneDragProps,
    hasActivity: playback.hasActivity,
    hasVideo,
    importedVideoDuration: playback.importedVideoDuration,
    importedVideoPath: playback.importedVideoPath,
    nudgeClip,
    selectedClipId,
    setSelectedClipId,
    videoSyncOffsetSeconds: timelineVideoSyncOffsetSeconds,
    viewEnd: viewport.viewport.viewEnd,
    viewStart: viewport.viewport.viewStart,
    widthPx: viewport.widthPx,
  })

  // Playhead geometry - converts the displayed playhead second into a stable pixel position for the surface.
  const pixelRatio = getDevicePixelRatio()
  const playheadLeft = roundToDevicePixel(
    secondsToViewPx({
      second: playback.clampedPlayhead,
      viewStart: viewport.viewport.viewStart,
      viewEnd: viewport.viewport.viewEnd,
      widthPx: viewport.widthPx,
    }),
    pixelRatio,
  )
  const snapGuidelineLeft =
    snapGuidelineSecond === null
      ? null
      : roundToDevicePixel(
          secondsToViewPx({
            second: snapGuidelineSecond,
            viewStart: viewport.viewport.viewStart,
            viewEnd: viewport.viewport.viewEnd,
            widthPx: viewport.widthPx,
          }),
          pixelRatio,
        )

  // Export marker models - hide markers outside the viewport and attach gesture props to visible handles.
  const exportMarkers = useMemo(
    () =>
      exportTimeline.markers
        .map((marker) => {
          const isVisible =
            marker.second >= timelineViewport.viewStart &&
            marker.second <= timelineViewport.viewEnd &&
            widthPx > 0 &&
            timelineViewport.viewEnd > timelineViewport.viewStart

          if (!isVisible) return null

          const left = roundToDevicePixel(
            secondsToViewPx({
              second: marker.second,
              viewStart: timelineViewport.viewStart,
              viewEnd: timelineViewport.viewEnd,
              widthPx,
            }),
            pixelRatio,
          )

          return {
            ...marker,
            handleClassName: getMarkerClassName(marker.marker),
            lineStyle: { left },
            markerProps: getExportMarkerProps(marker.marker),
            style: { left },
          }
        })
        .filter(Boolean),
    [exportTimeline.markers, getExportMarkerProps, pixelRatio, timelineViewport.viewEnd, timelineViewport.viewStart, widthPx],
  )

  // Fit target commands - presentational tabs only need active state, labels, and a command callback.
  const fitTargets = useMemo(
    () =>
      viewportFitTargets.map((target) => ({
        id: target.id,
        isActive: displayedFitTargetId === target.id,
        label: target.label,
        onSelect: () => fitTarget(target.id),
      })),
    [displayedFitTargetId, fitTarget, viewportFitTargets],
  )

  // View model - components receive only render-ready state and callbacks, not store or calculation details.
  return {
    isVisible: playback.hasActivity || hasVideo,
    timeline: {
      axisProps: gestures.axisProps,
      containerProps: {
        onWheel: viewport.handleWheel,
        ref: viewport.containerRef,
      },
      exportMarkers,
      lanes,
      panSurfaceProps: gestures.panSurfaceProps,
      playhead: {
        handleProps: gestures.playheadProps,
        lineStyle: { left: playheadLeft },
        style: { left: playheadLeft },
      },
      snapGuidelineStyle: snapGuidelineLeft === null ? null : { left: snapGuidelineLeft },
      ticks: viewport.ticks,
      viewport: viewport.viewport,
      widthPx: viewport.widthPx,
    },
    toolbar: {
      exportRange: {
        clear: exportTimeline.clear,
        isCustom: exportTimeline.isCustom,
        isDisabled: !hasActivityData && !hasVideo,
        label: exportTimeline.rangeLabel,
        setEnd: () => exportTimeline.setBoundary('to', exportBoundarySecond),
        setStart: () => exportTimeline.setBoundary('from', exportBoundarySecond),
      },
      fitTargets,
      resetView: {
        disabled: viewport.isFullTimelineVisible,
        onClick: viewport.resetView,
      },
      timeLabel: {
        current: formatTimelineTime(playback.clampedPlayhead),
        total: formatTimelineTime(playback.totalDuration),
      },
      hasVideo,
      isMuted: playerStore.isVideoMuted,
      toggleMute: playerStore.toggleVideoMute,
      transport: {
        isDisabled: !playback.hasActivity,
        isPlaying: playback.isPlaying,
        jumpToEnd: playback.jumpToEnd,
        pause: playback.pause,
        play: playback.play,
        resetToStart: playback.resetToStart,
        stepBackward: () => playback.stepBySeconds(-1),
        stepForward: () => playback.stepBySeconds(1),
      },
      zoomIn: viewport.zoomIn,
      zoomOut: viewport.zoomOut,
    },
  }
}
