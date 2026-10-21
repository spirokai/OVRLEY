/**
 * Top-level player orchestration hook for the presentational overlay player components.
 */

import { useEffect, useEffectEvent, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { hasOpenOverlay, isFormFieldShortcut, matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { formatClockDuration } from '@/lib/time-format'
import { resolvePreviewTarget } from '@/lib/video-program/playback-target'
import { useJoinTransitionModels } from '@/features/transitions'
import { useVideoTimeline, useVideoWarnings } from '@/hooks/useVideoTimeline'
import useStore from '@/store/useStore'
import { snapTimelineSecondToFrame } from '../utils/playerTiming'
import { roundToDevicePixel, secondsToViewPx } from '../utils/timelineGeometry'

import useClipDrag from './useClipDrag'
import useDeletedPartDrag from './useDeletedPartDrag'
import useDeletedPartMarking from './useDeletedPartMarking'
import useExportRangeTimeline from './useExportRangeTimeline'
import usePlaybackEngine from './usePlaybackEngine'
import useTimelineClips from './useTimelineClips'
import useTimelineGestures from './useTimelineGestures'
import useTimelineViewport from './useTimelineViewport'
import useVideoSyncTimeline from '@/features/video-sync/hooks/useVideoSyncTimeline'

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
 * @param {{ activeKeyboardWorkspace: string, backgroundMode: string, videoSyncMode?: boolean }} options Overlay player inputs.
 * @param {string} options.activeKeyboardWorkspace Active keyboard command workspace.
 * @param {string} options.backgroundMode Active preview background mode.
 * @param {function} [options.onRemoveVideo] Removes a video clip by video ID.
 * @returns {object} Presentational view model for the overlay player.
 */
export default function useOverlayPlayer({ activeKeyboardWorkspace, backgroundMode, onRemoveVideo, videoSyncMode = false }) {
  // Store selector - gathers the entire player-facing store contract in one subscription.
  const playerStore = useStore(
    useShallow((state) => ({
      activityFilename: state.activitySource?.path?.split(/[/\\]/).at(-1) ?? state.activitySummary?.fileName ?? null,
      activitySummary: state.activitySummary,
      addDeletedPart: state.addDeletedPart,
      beginPreviewScrub: state.beginPreviewScrub,
      deletedPartMark: state.deletedPartMark,
      commitPreviewScrub: state.commitPreviewScrub,
      fallbackDurationSeconds: state.fallbackDurationSeconds,
      hasVideoTelemetry: state.importedVideos.some((video) => video.telemetry.status === 'ready'),
      manualVideoSyncDetection: state.manualVideoSyncDetection,
      manualVideoSyncLandmarks: state.manualVideoSync.landmarks,
      manualVideoSyncVideoId: state.manualVideoSync.videoId,
      moveVideoSyncLandmark: state.moveVideoSyncLandmark,
      pausePreviewPlayback: state.pausePreviewPlayback,
      toggleVideoMute: state.toggleVideoMute,
      isVideoMuted: state.isVideoMuted,
      previewPlaybackSource: state.previewPlaybackSource,
      previewPlaybackState: state.previewPlaybackState,
      sceneFps: state.renderSettings.fps,
      selectedSecond: state.selectedSecond,
      setSelectedSecond: state.setSelectedSecond,
      setVideoDragPreview: state.setVideoDragPreview,
      setVideoOffset: state.setVideoOffset,
      shiftVideoOffsets: state.shiftVideoOffsets,
      startPreviewPlayback: state.startPreviewPlayback,
      selectedWidgetIds: state.selectedWidgetIds,
      updatePreviewScrub: state.updatePreviewScrub,
      updateRate: state.renderSettings.widgetUpdateRate,
    })),
  )
  const timeline = useVideoTimeline()
  const videoWarnings = useVideoWarnings()
  const displayedTimeline = timeline.displayed
  const joinModels = useJoinTransitionModels(displayedTimeline)
  const videoIdentity = useMemo(() => timeline.clips.map((clip) => `${clip.id}:${clip.durationSeconds}`).join('|'), [timeline.clips])

  const [selectedClipId, setSelectedClipId] = useState(null)

  // Media flags - downstream hooks need explicit availability booleans, not inferred path/summary checks.
  const hasVideo = timeline.clips.length > 0
  const hasActivityData = Boolean(playerStore.activitySummary)
  const activityDurationSeconds = playerStore.activitySummary?.durationSeconds ?? 0
  const canSelectClip = hasVideo

  // Durable domains - each hook owns behavior for one player concern, while this hook wires them together.
  const playback = usePlaybackEngine({
    ...playerStore,
    activityDurationSeconds,
    backgroundMode,
    hasActivityData: hasActivityData || playerStore.hasVideoTelemetry,
    program: timeline.program,
    videoBounds: timeline.bounds,
    videoIdentity,
  })
  const { hasActivity, isPlaying, pause, play, stepBySeconds } = playback
  const boundaryTarget = resolvePreviewTarget(timeline.program, playback.clampedPlayhead)
  const boundaryClip = boundaryTarget === null ? null : timeline.clips.find((clip) => clip.id === boundaryTarget.videoId)
  const exportBoundarySecond =
    boundaryClip === null
      ? playback.clampedPlayhead
      : snapTimelineSecondToFrame(playback.clampedPlayhead, boundaryClip.fps.num / boundaryClip.fps.den, boundaryClip.offsetSeconds)
  const exportTimeline = useExportRangeTimeline({
    defaultEndSecond: timeline.bounds?.endSecond ?? playback.totalDuration,
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

  // Selection is scoped to the current media and ends when the user clicks elsewhere.
  useEffect(() => {
    setSelectedClipId(null)
  }, [playerStore.activitySummary, videoIdentity])

  useEffect(() => {
    const clearClipSelection = () => setSelectedClipId(null)
    window.addEventListener('pointerdown', clearClipSelection, true)
    return () => window.removeEventListener('pointerdown', clearClipSelection, true)
  }, [])

  // Clip drag - owns horizontal drag gestures and keyboard nudges for clip offsets.
  const clipDrag = useClipDrag({
    setVideoDragPreview: playerStore.setVideoDragPreview,
    setVideoOffset: playerStore.setVideoOffset,
    shiftVideoOffsets: playerStore.shiftVideoOffsets,
  })
  const deletedPartDrag = useDeletedPartDrag({ addDeletedPart: playerStore.addDeletedPart })
  const deletedPartMarking = useDeletedPartMarking({
    clips: timeline.clips,
    program: timeline.program,
    playheadSecond: playback.clampedPlayhead,
    selectedClipId,
  })
  const {
    commitNudge: commitClipNudge,
    getLaneDragProps,
    isDragging: isClipDragging,
    nudgeLane: nudgeClip,
    snapGuidelineSecond,
    updateMetrics: updateClipDragMetrics,
  } = clipDrag

  // Viewport domain - owns measurement, fit targets, ticks, zoom, pan, and playback follow behavior.
  const viewport = useTimelineViewport({
    activityDurationSeconds,
    displayedVideoBounds: displayedTimeline.bounds,
    fallbackDurationSeconds: playerStore.fallbackDurationSeconds,
    hasActivityData,
    isDragging: gestures.isTimelineDragging || isClipDragging,
    isPlaying: playback.isPlaying,
    playheadSecond: playback.clampedPlayhead,
    totalDuration: playback.totalDuration,
    videoBounds: timeline.bounds,
    videoIdentity,
  })
  const { displayedFitTargetId, fitTarget, fitTargets: viewportFitTargets, viewport: timelineViewport, widthPx } = viewport

  // Sync timeline - the feature hook receives the already-owned player viewport
  // and returns only render-ready graph and landmark geometry.
  const syncTargetClip = timeline.clips.find((clip) => clip.id === playerStore.manualVideoSyncVideoId) ?? null
  const displayedSyncTargetClip = displayedTimeline.clips.find((clip) => clip.id === playerStore.manualVideoSyncVideoId) ?? null
  const videoSyncTimeline = useVideoSyncTimeline({
    containerElement: viewport.containerElement,
    detection: playerStore.manualVideoSyncDetection,
    enabled: videoSyncMode,
    followSecond: viewport.followSecond,
    landmarks: playerStore.manualVideoSyncLandmarks,
    moveLandmark: playerStore.moveVideoSyncLandmark,
    scrubTo: playback.scrubTo,
    timelineMinimum: viewport.timelineMinimum,
    totalDuration: playback.totalDuration,
    videoDuration: syncTargetClip?.durationSeconds ?? null,
    videoOffsetSeconds: syncTargetClip?.offsetSeconds ?? 0,
    displayedVideoOffsetSeconds: displayedSyncTargetClip?.offsetSeconds ?? 0,
    viewEnd: viewport.viewport.viewEnd,
    viewStart: viewport.viewport.viewStart,
    widthPx: viewport.widthPx,
  })

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
      activityDurationSeconds,
      clips: timeline.clips,
      hasActivityData,
    })
  }, [
    activityDurationSeconds,
    hasActivityData,
    playback.totalDuration,
    timeline.clips,
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
      case 'deletedPart.markStart':
        if (videoSyncMode || !deletedPartMarking.canStart) return
        event.preventDefault()
        deletedPartMarking.start()
        return
      case 'deletedPart.markEnd':
        if (videoSyncMode || !deletedPartMarking.canEnd) return
        event.preventDefault()
        deletedPartMarking.end()
        return
      case 'deletedPart.clearMark':
        if (deletedPartMarking.mark === null) return
        event.preventDefault()
        deletedPartMarking.clear()
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
        playerStore.shiftVideoOffsets(match.binding.seconds)
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
    clips: displayedTimeline.clips,
    commitClipNudge,
    deletedPartDrag,
    deletedPartMark: playerStore.deletedPartMark,
    exportHighlightRange: videoSyncMode ? null : exportTimeline.highlightRange,
    getLaneDragProps,
    hasActivity: hasActivityData && playback.hasActivity,
    joinModels,
    nudgeClip,
    onRemoveVideo,
    overlaps: displayedTimeline.program.overlaps,
    program: videoSyncMode ? null : displayedTimeline.program,
    selectedClipId,
    setSelectedClipId,
    warnings: videoWarnings,
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
  const exportMarkers = useMemo(() => {
    if (videoSyncMode) return []

    return exportTimeline.markers
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
      .filter(Boolean)
  }, [exportTimeline.markers, getExportMarkerProps, pixelRatio, timelineViewport.viewEnd, timelineViewport.viewStart, videoSyncMode, widthPx])

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
      graph: videoSyncTimeline.graph,
      lanes,
      landmarks: videoSyncTimeline.landmarks,
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
      videoSyncMode,
    },
    toolbar: {
      deletedPart: {
        isVisible: hasVideo,
        canStart: deletedPartMarking.canStart,
        canEnd: deletedPartMarking.canEnd,
        markLabel: deletedPartMarking.mark?.label ?? null,
        start: deletedPartMarking.start,
        end: deletedPartMarking.end,
        clear: deletedPartMarking.clear,
      },
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
        current: formatClockDuration(playback.clampedPlayhead),
        total: formatClockDuration(playback.totalDuration),
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
