/**
 * Top-level player orchestration hook for the presentational overlay player components.
 */

import { useCallback, useEffect, useEffectEvent, useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { hasOpenOverlay, isFormFieldShortcut, matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { useJoinTransitionModels } from '@/features/transitions'
import { useVideoTimeline, useVideoWarnings } from '@/hooks/useVideoTimeline'
import useStore from '@/store/useStore'
import { PLAYHEAD_CENTER_X } from '../data/playheadGeometry'
import { applyCutDraft, applyCutDraftToSelection, segmentOfCut } from '../utils/cutDrag'
import { roundToDevicePixel, secondsToViewPx, snapToScreenPixel, videoIdFromLaneId, videoLaneId } from '../utils/timelineGeometry'
import { resolveFitVideoClip } from '../utils/timelineViewport'

import useActivityTrim from './useActivityTrim'
import useClipDrag from './useClipDrag'
import useCutDrag from './useCutDrag'
import usePlaybackEngine from './usePlaybackEngine'
import useTimelineClips from './useTimelineClips'
import useTimelineGestures from './useTimelineGestures'
import useTimelineViewport from './useTimelineViewport'
import useVideoCuts from './useVideoCuts'
import useWantedFilmstripWindows from './useWantedFilmstripWindows'
import useVideoSyncTimeline from '@/features/video-sync/hooks/useVideoSyncTimeline'

/** Elements whose clicks keep the clip selection: kept areas, and the portaled popups and dialogs they open. */
const KEEPS_CLIP_SELECTION = '[data-keeps-clip-selection], [data-radix-popper-content-wrapper], [role="dialog"]'

/**
 * Composes store selection, playback, viewport, activity trim, gestures, clips, and keyboard commands.
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
      beginPreviewScrub: state.beginPreviewScrub,
      commitPreviewScrub: state.commitPreviewScrub,
      fallbackDurationSeconds: state.fallbackDurationSeconds,
      hasVideoTelemetry: state.importedVideos.some((video) => video.telemetry.status === 'ready'),
      manualVideoSyncDetection: state.manualVideoSyncDetection,
      manualVideoSyncLandmarks: state.manualVideoSync.landmarks,
      manualVideoSyncVideoId: state.manualVideoSync.videoId,
      moveVideoSyncLandmark: state.moveVideoSyncLandmark,
      pausePreviewPlayback: state.pausePreviewPlayback,
      moveCut: state.moveCut,
      trimVideo: state.trimVideo,
      requestCutRemoval: state.requestCutRemoval,
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
      videoFilmstrips: state.videoFilmstrips,
    })),
  )
  const timeline = useVideoTimeline()
  const videoWarnings = useVideoWarnings()
  const displayedTimeline = timeline.displayed
  const joinModels = useJoinTransitionModels(displayedTimeline)
  const videoIdentity = useMemo(() => timeline.clips.map((clip) => `${clip.id}:${clip.durationSeconds}`).join('|'), [timeline.clips])

  // Clip selection - a clip, and on a video clip the segment under the pointer (null when selected from the keyboard).
  // The store owns it, so the video panel and the video drawer show the same selection.
  const selectedClip = useStore((state) => state.selectedClip)
  const selectClip = useStore((state) => state.selectClip)

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
  const { hasActivity, isPlaying, pause, play, readPlayheadSecond, stepBySeconds } = playback
  const gestures = useTimelineGestures({
    cancelScrub: playback.cancelScrub,
    commitScrub: playback.commitScrub,
    scrubTo: playback.scrubTo,
  })
  const { updateTimelineMetrics } = gestures

  // Selection ends when the user clicks outside the timeline, its toolbar, the video panel, and the video drawer;
  // the store ends it when its clip leaves the timeline. Pickers, menus, and dialogs are portaled outside those
  // areas but belong to the control that opened them, so using them keeps the selection.
  useEffect(() => {
    const clearClipSelection = (event) => {
      if (useStore.getState().selectedClip !== null && !event.target.closest?.(KEEPS_CLIP_SELECTION)) selectClip(null)
    }
    window.addEventListener('pointerdown', clearClipSelection, true)
    return () => window.removeEventListener('pointerdown', clearClipSelection, true)
  }, [selectClip])

  // Clip drag - owns horizontal drag gestures and keyboard nudges for clip offsets.
  const clipDrag = useClipDrag({
    setVideoDragPreview: playerStore.setVideoDragPreview,
    setVideoOffset: playerStore.setVideoOffset,
    shiftVideoOffsets: playerStore.shiftVideoOffsets,
  })
  const videoCuts = useVideoCuts({
    clips: timeline.clips,
    program: timeline.program,
    playheadSecond: playback.clampedPlayhead,
    readPlayheadSecond,
    selectedClip,
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
  // Video fit target - one clip (selected, at the playhead, or nearest to it), kept by value so playback frames
  // do not rebuild the fit targets.
  const fitVideoClip = resolveFitVideoClip(
    timeline.clips,
    selectedClip === null ? null : videoIdFromLaneId(selectedClip.laneId),
    playback.clampedPlayhead,
  )
  const fitVideoStart = fitVideoClip?.offsetSeconds ?? null
  const fitVideoEnd = fitVideoClip === null ? null : fitVideoClip.offsetSeconds + fitVideoClip.durationSeconds
  const videoClipRange = useMemo(
    () => (fitVideoStart === null ? null : { startSecond: fitVideoStart, endSecond: fitVideoEnd }),
    [fitVideoEnd, fitVideoStart],
  )
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
    videoClipRange,
    videoIdentity,
  })
  const { displayedFitTargetId, fitTarget, fitTargets: viewportFitTargets } = viewport

  // Activity trim - edge handles on the activity lane and the trim shortcuts.
  const activityTrim = useActivityTrim({
    durationSeconds: activityDurationSeconds,
    viewEnd: viewport.viewport.viewEnd,
    viewStart: viewport.viewport.viewStart,
    widthPx: viewport.widthPx,
  })

  // Trim at the playhead - the toolbar's [ ] and their shortcuts trim the video at the playhead, or the activity
  // without videos: the part before the playhead (start) or after it (end).
  const { setEdge: setActivityTrimEdge } = activityTrim
  const { canTrim: canTrimVideo, trim: trimVideoAtPlayhead } = videoCuts
  const canTrimStart = !videoSyncMode && (hasVideo ? canTrimVideo.start : hasActivityData)
  const canTrimEnd = !videoSyncMode && (hasVideo ? canTrimVideo.end : hasActivityData)
  const trimAtPlayhead = useCallback(
    (side) => (hasVideo ? trimVideoAtPlayhead(side) : setActivityTrimEdge(side, readPlayheadSecond())),
    [hasVideo, readPlayheadSecond, setActivityTrimEdge, trimVideoAtPlayhead],
  )

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
      case 'trim.start':
        if (!canTrimStart) return
        event.preventDefault()
        trimAtPlayhead('start')
        return
      case 'trim.end':
        if (!canTrimEnd) return
        event.preventDefault()
        trimAtPlayhead('end')
        return
      case 'activityTrim.reset':
        if (activityTrim.trim === null || videoSyncMode) return
        event.preventDefault()
        activityTrim.reset()
        return
      case 'activityTrim.goToStart':
        if (!hasActivityData) return
        event.preventDefault()
        playback.commitScrub(activityTrim.window.startSecond)
        return
      case 'activityTrim.goToEnd':
        if (!hasActivityData) return
        event.preventDefault()
        playback.commitScrub(activityTrim.window.endSecond)
        return
      case 'videoEdit.cut':
        if (videoSyncMode || !videoCuts.canCut) return
        event.preventDefault()
        videoCuts.cut()
        return
      case 'videoEdit.toggleExcluded':
        if (videoSyncMode || videoCuts.excluded === null) return
        event.preventDefault()
        videoCuts.toggleExcluded()
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

  // Cut drag - the dragged cut is shown at its draft frame until it is committed on release. Cut markers keep the
  // selection on press, so a press keeps its clip selected, or selects the pressed cut's segment instead.
  const selectCutClip = useCallback(
    (clip, cut) => {
      const laneId = videoLaneId(clip.id)
      if (useStore.getState().selectedClip?.laneId === laneId) return
      selectClip({ laneId, segmentKey: segmentOfCut(clip, cut.id) })
    },
    [selectClip],
  )
  const cutDrag = useCutDrag({
    moveCut: playerStore.moveCut,
    removeCut: playerStore.requestCutRemoval,
    selectCutClip,
    trimVideo: playerStore.trimVideo,
    viewEnd: viewport.viewport.viewEnd,
    viewStart: viewport.viewport.viewStart,
    widthPx: viewport.widthPx,
  })
  const timelineClips = useMemo(() => applyCutDraft(displayedTimeline.clips, cutDrag.draft), [displayedTimeline.clips, cutDrag.draft])

  // Lane models - clip geometry and tooltip state are prepared before presentational rendering.
  const lanes = useTimelineClips({
    activityFilename: playerStore.activityFilename,
    activitySummary: playerStore.activitySummary,
    canSelect: canSelectClip,
    clips: timelineClips,
    commitClipNudge,
    getCutDragProps: cutDrag.getCutDragProps,
    draggedCut: cutDrag.draft,
    filmstrips: playerStore.videoFilmstrips,
    activityTrim: videoSyncMode ? null : activityTrim,
    getLaneDragProps,
    hasActivity: hasActivityData && playback.hasActivity,
    joinModels,
    nudgeClip,
    onRemoveCut: playerStore.requestCutRemoval,
    onRemoveVideo,
    overlaps: displayedTimeline.program.overlaps,
    program: videoSyncMode ? null : displayedTimeline.program,
    selectedClip: applyCutDraftToSelection(selectedClip, cutDrag.draft),
    setSelectedClip: selectClip,
    warnings: videoWarnings,
    viewEnd: viewport.viewport.viewEnd,
    viewStart: viewport.viewport.viewStart,
    widthPx: viewport.widthPx,
  })
  useWantedFilmstripWindows({
    clips: timelineClips,
    viewEnd: viewport.viewport.viewEnd,
    viewStart: viewport.viewport.viewStart,
    widthPx: viewport.widthPx,
  })

  // Playhead geometry - places the playhead SVG so its line center sits on the displayed second, with the SVG origin
  // snapped to a screen pixel so every position renders with the same sub-pixel phase.
  const pixelRatio = viewport.pixelGrid.ratio
  const playheadOffset = snapToScreenPixel(
    secondsToViewPx({
      second: playback.clampedPlayhead,
      viewStart: viewport.viewport.viewStart,
      viewEnd: viewport.viewport.viewEnd,
      widthPx: viewport.widthPx,
    }) - PLAYHEAD_CENTER_X,
    viewport.pixelGrid,
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

  // Toolbar view model - memoized without the playhead so the toolbar skips playback frames; the time label
  // subscribes to the playhead itself.
  const { jumpToEnd, resetToStart, timelineMinimum, totalDuration } = playback
  const { isVideoMuted, toggleVideoMute } = playerStore
  const { canZoomIn, canZoomOut, isFullTimelineVisible, resetView, zoomIn, zoomOut } = viewport
  const { canCut, cut: cutAtPlayhead, excluded: excludedAtPlayhead, toggleExcluded: toggleExcludedAtPlayhead } = videoCuts
  const toolbar = useMemo(
    () => ({
      videoEdit: {
        isVisible: hasVideo,
        canCut,
        cut: cutAtPlayhead,
        excluded: excludedAtPlayhead,
        toggleExcluded: toggleExcludedAtPlayhead,
      },
      trim: {
        isVisible: !videoSyncMode && (hasVideo || hasActivityData),
        target: hasVideo ? 'video' : 'activity',
        canTrimStart,
        canTrimEnd,
        trimStart: () => trimAtPlayhead('start'),
        trimEnd: () => trimAtPlayhead('end'),
      },
      fitTargets,
      resetView: {
        disabled: isFullTimelineVisible,
        onClick: resetView,
      },
      timeLabel: {
        timelineMinimum,
        totalDuration,
      },
      hasVideo,
      isMuted: isVideoMuted,
      toggleMute: toggleVideoMute,
      transport: {
        isDisabled: !hasActivity,
        isPlaying,
        jumpToEnd,
        pause,
        play,
        resetToStart,
        stepBackward: () => stepBySeconds(-1),
        stepForward: () => stepBySeconds(1),
      },
      zoomIn: { disabled: !canZoomIn, onClick: zoomIn },
      zoomOut: { disabled: !canZoomOut, onClick: zoomOut },
    }),
    [
      canCut,
      canZoomIn,
      canZoomOut,
      canTrimEnd,
      canTrimStart,
      cutAtPlayhead,
      excludedAtPlayhead,
      fitTargets,
      hasActivity,
      hasActivityData,
      hasVideo,
      isFullTimelineVisible,
      isPlaying,
      isVideoMuted,
      jumpToEnd,
      pause,
      play,
      resetToStart,
      resetView,
      stepBySeconds,
      timelineMinimum,
      toggleExcludedAtPlayhead,
      toggleVideoMute,
      totalDuration,
      trimAtPlayhead,
      videoSyncMode,
      zoomIn,
      zoomOut,
    ],
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
      graph: videoSyncTimeline.graph,
      lanes,
      landmarks: videoSyncTimeline.landmarks,
      panSurfaceProps: gestures.panSurfaceProps,
      playhead: {
        handleProps: gestures.playheadProps,
        // Positioned by transform because layout snaps `left` to whole CSS pixels before the app shell's scale,
        // which lands on a different sub-pixel phase at each step.
        style: { transform: `translateX(${playheadOffset}px)` },
      },
      snapGuidelineStyle: snapGuidelineLeft === null ? null : { left: snapGuidelineLeft },
      ticks: viewport.ticks,
      viewport: viewport.viewport,
      widthPx: viewport.widthPx,
      videoSyncMode,
    },
    toolbar,
  }
}
