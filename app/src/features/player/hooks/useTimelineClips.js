/**
 * Builds render-ready lane models: one video lane holding every clip with its
 * overlap bands, and the activity lane.
 */

import { useCallback, useId, useMemo, useState } from 'react'
import { AlertTriangle, Video } from 'lucide-react'
import { filenameFromSelectedPath } from '@/lib/file-dialog'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { formatClockDuration } from '@/lib/time-format'
import { edgeCutSide, listEdgeCuts } from '../utils/cutDrag'
import { listFilmstripTiles } from '../utils/filmstripTiles'
import { ACTIVITY_LANE_ID, getClipGeometry, secondsToViewPx, videoLaneId } from '../utils/timelineGeometry'
import { getActivityAttributeLabel } from '@/lib/widget/widget-presentation'
import { framesToSeconds } from '@/lib/video-program/frame-grid'
import { sortVideosByTimeline } from '@/lib/video-program/video-collection'
import { formatVideoWarning } from '@/lib/video-program/video-warnings'
import { segmentBeforeCut } from '@/lib/video-program/collection-edits'
import { listVideoSegments } from '@/lib/video-program/video-segments'
import i18next from 'i18next'
import { skippedDefaultLabel } from '@/features/transitions'

const TEXT_HIDE_THRESHOLD_REM = 3
/** Height of a video clip: the video lane's `h-12`. Filmstrip tiles are this tall. */
const VIDEO_CLIP_HEIGHT_REM = 3
const EDGE_CUT_MIN_CLIP_REM = 3
const CLIP_SOURCE_COLUMN_WIDTH = '3rem'
const CLIP_CONTENT_OFFSET_CLASS = 'translate-y-[0.04rem]'
const CLIP_SYNC_STEP_SECONDS = 0.1

function getRootRemPx() {
  if (typeof window === 'undefined') return 16
  return Number.parseFloat(window.getComputedStyle?.(document.documentElement).fontSize) || 16
}

function clampPx(value, min, max) {
  return Math.min(max, Math.max(min, value))
}

function clipRangeStyle(fromSecond, toSecond, durationSeconds) {
  return { left: `${(fromSecond / durationSeconds) * 100}%`, width: `${((toSecond - fromSecond) / durationSeconds) * 100}%` }
}

/**
 * Segment blocks of a clip, positioned within it, with a seam at each cut and the visible tiles of the clip's filmstrip.
 *
 * @param {object} clip Displayed video clip.
 * @param {string|null} selectedSegmentKey Selected segment of the clip.
 * @param {{filmstrip: object, clipLeftPx: number, pxPerSecond: number, tileWidthPx: number, widthPx: number}|null} filmstripView
 *   The clip's ready filmstrip and its placement in the viewport, or null when the clip shows no filmstrip.
 */
function segmentModels(clip, selectedSegmentKey, filmstripView) {
  const segments = listVideoSegments(clip)
  return segments.map((segment, index) => {
    const sourceStartSecond = framesToSeconds(segment.startFrame, clip.fps)
    const sourceEndSecond = framesToSeconds(segment.endFrame, clip.fps)
    return {
      key: segment.key,
      excluded: segment.excluded,
      missing: clip.missing,
      hasSeamBefore: index > 0,
      hasSeamAfter: index < segments.length - 1,
      isSelected: segment.key === selectedSegmentKey,
      style: clipRangeStyle(sourceStartSecond, sourceEndSecond, clip.durationSeconds),
      tiles:
        filmstripView === null
          ? []
          : listFilmstripTiles({
              filmstrip: filmstripView.filmstrip,
              sourceStartSecond,
              sourceEndSecond,
              leftPx: filmstripView.clipLeftPx + sourceStartSecond * filmstripView.pxPerSecond,
              pxPerSecond: filmstripView.pxPerSecond,
              tileWidthPx: filmstripView.tileWidthPx,
              widthPx: filmstripView.widthPx,
            }),
    }
  })
}

/** Filmstrip placement of a video clip, or null when its filmstrip is not ready or the clip is off screen. */
function clipFilmstripView(clip, filmstrip, geometry, { viewStart, viewEnd, widthPx }) {
  if (filmstrip?.status !== 'ready' || !geometry.isVisible) return null
  return {
    filmstrip,
    clipLeftPx: geometry.x,
    pxPerSecond: widthPx / (viewEnd - viewStart),
    tileWidthPx: Math.round((VIDEO_CLIP_HEIGHT_REM * getRootRemPx() * clip.resolution.width) / clip.resolution.height),
    widthPx,
  }
}

/**
 * Striped masks over the visible parts of the activity outside its trim, positioned within the activity clip.
 * Each is cut to the viewport: zoomed in, the clip is far wider than the screen, and a striped element that
 * wide is not drawn.
 */
function activityTrimMasks(window, durationSeconds, viewStart, viewEnd) {
  if (!window.isTrimmed || durationSeconds <= 0) return []
  return [
    { key: 'start', fromSecond: 0, toSecond: window.startSecond },
    { key: 'end', fromSecond: window.endSecond, toSecond: durationSeconds },
  ]
    .map((mask) => ({ ...mask, fromSecond: Math.max(mask.fromSecond, viewStart), toSecond: Math.min(mask.toSecond, viewEnd) }))
    .filter((mask) => mask.toSecond > mask.fromSecond)
    .map((mask) => ({ key: mask.key, style: clipRangeStyle(mask.fromSecond, mask.toSecond, durationSeconds) }))
}

function activityGeometryWidthPx(durationSeconds, viewStart, viewEnd, widthPx) {
  return getClipGeometry({ startSecond: 0, durationSeconds, viewStart, viewEnd, widthPx }).width
}

/** Segment under a pointer event on a video clip, or null for a keyboard press on the clip itself. */
function segmentKeyFromEvent(event) {
  return event.target.closest?.('[data-segment-key]')?.dataset.segmentKey ?? null
}

function percentStyle(geometry, widthPx) {
  return {
    left: widthPx > 0 ? `${(geometry.x / widthPx) * 100}%` : '0%',
    width: widthPx > 0 ? `${(geometry.width / widthPx) * 100}%` : '0%',
  }
}

/**
 * Owns lane and clip view models, tooltip state, clip geometry, overlap bands, and the activity trim.
 *
 * @param {object} options Clip timeline inputs.
 * @param {string|null} options.activityFilename Imported activity filename.
 * @param {object|null} options.activitySummary External activity summary metadata.
 * @param {{window: {startSecond: number, endSecond: number, isTrimmed: boolean}, draggedEdge: 'start'|'end'|null, getEdgeDragProps: function}|null} options.activityTrim
 *   Displayed activity trim with its edge handle props (`useActivityTrim`), or null where the trim is not edited.
 * @param {boolean} options.canSelect Whether clips can be selected for keyboard nudges.
 * @param {Array<object>} options.clips Displayed timeline clips.
 * @param {function} options.commitClipNudge Callback to commit a keyboard sync adjustment.
 * @param {{videoId: string, cutId: string, frame: number}|null} options.draggedCut Cut being dragged, edge cuts included (`useCutDrag`), or null.
 * @param {Object<string, object>} options.filmstrips Timeline filmstrips keyed by video path (`videoFilmstrips` in the store).
 * @param {function} options.getCutDragProps Cut marker drag handler factory from useCutDrag (edge cuts included).
 * @param {function} options.getLaneDragProps Per-lane drag handler factory from useClipDrag.
 * @param {boolean} options.hasActivity Whether the external activity lane should be present.
 * @param {Map<string, object>} [options.joinModels] Transition controls keyed by join key (`useJoinTransitionModels`).
 * @param {function} options.nudgeClip Callback to move a clip by a relative number of seconds.
 * @param {function} [options.onRemoveCut] Callback to remove a cut by video ID and cut ID.
 * @param {function} [options.onRemoveVideo] Callback to remove a video clip by video ID.
 * @param {Array<{startSecond: number, endSecond: number}>} [options.overlaps] Overlapping timeline intervals of the displayed clips.
 * @param {{pieces: Array<object>, joins: Array<object>}} [options.program] Displayed program, for its join markers.
 * @param {{laneId: string, segmentKey: string|null}|null} options.selectedClip Selected clip, and segment of a video clip.
 * @param {function} options.setSelectedClip Callback to set the selection.
 * @param {number} options.viewEnd Visible viewport end second.
 * @param {number} options.viewStart Visible viewport start second.
 * @param {Map<string, Array<object>>} [options.warnings] Clip warnings keyed by video ID (`useVideoWarnings`).
 * @param {number} options.widthPx Measured timeline width.
 * @returns {Array<object>} Presentational lane view models, each with its clips.
 */
export default function useTimelineClips({
  activityFilename,
  activitySummary,
  activityTrim,
  canSelect,
  clips,
  commitClipNudge,
  draggedCut,
  filmstrips,
  getCutDragProps,
  getLaneDragProps,
  hasActivity,
  joinModels,
  nudgeClip,
  onRemoveCut,
  onRemoveVideo,
  overlaps = [],
  program,
  selectedClip,
  setSelectedClip,
  viewEnd,
  viewStart,
  warnings,
  widthPx,
}) {
  // Tooltip identity/state - one hovered lane controls the out-of-mask tooltip renderer.
  const tooltipBaseId = useId()
  const [hoveredLaneId, setHoveredLaneId] = useState(null)

  // Clip events - clips show hover detail but must not begin timeline pan or scrub interactions.
  const stopClipEvent = useCallback((event) => {
    event.stopPropagation()
  }, [])

  // Lane view models - build video/activity lane inputs first, then attach geometry and handlers.
  return useMemo(() => {
    const laneInputs = []
    const activityDurationSeconds = activitySummary?.durationSeconds ?? 0

    // Video clips - each clip starts at its offset so clip geometry represents real timeline placement.
    for (const clip of sortVideosByTimeline(clips)) {
      laneInputs.push({
        clip,
        durationSeconds: clip.durationSeconds,
        formatLabel: 'MP4',
        icon: clip.missing ? AlertTriangle : Video,
        id: videoLaneId(clip.id),
        isVideo: true,
        label: filenameFromSelectedPath(clip.path),
        startSecond: clip.offsetSeconds,
        warnings: (warnings?.get(clip.id) ?? []).map((warning) => formatVideoWarning(warning, i18next.t)),
      })
    }

    // Activity lane - always starts at zero and uses external activity metadata for label/duration.
    if (hasActivity) {
      const availableMetrics = activitySummary.availableMetrics
        .map((metric) => getActivityAttributeLabel(metric.attribute, i18next.t))
        .sort((left, right) => left.localeCompare(right))

      laneInputs.push({
        availableMetrics,
        durationSeconds: activityDurationSeconds,
        formatLabel: activitySummary.fileFormat?.toUpperCase() || 'DATA',
        icon: null,
        id: ACTIVITY_LANE_ID,
        isVideo: false,
        label: activityFilename || activitySummary?.fileName || 'Activity',
        startSecond: 0,
        warnings: [],
      })
    }

    const clipModels = laneInputs.map((lane) => {
      const dragProps = getLaneDragProps(lane.id)
      // Clip geometry - percent styles are derived from pixel geometry so rendering stays responsive.
      const geometry = getClipGeometry({
        startSecond: lane.startSecond,
        durationSeconds: lane.durationSeconds,
        viewStart,
        viewEnd,
        widthPx,
      })

      // Tooltip position - anchor to the visible portion of partially clipped lanes.
      const visibleStartPx = geometry.isVisible ? clampPx(geometry.x, 0, widthPx) : 0
      const visibleEndPx = geometry.isVisible ? clampPx(geometry.x + geometry.width, 0, widthPx) : 0
      const tooltipAnchorPx = visibleStartPx + Math.max(0, visibleEndPx - visibleStartPx) / 2
      const showTooltip = hoveredLaneId === lane.id && Boolean(lane.label)
      const isSelected = selectedClip?.laneId === lane.id
      const selectedSegmentKey = isSelected ? selectedClip.segmentKey : null
      // Text span - the clip's text sits over the untrimmed part of a trimmed activity.
      const textWindow =
        !lane.isVideo && activityTrim?.window.isTrimmed === true && lane.durationSeconds > 0
          ? { startSecond: activityTrim.window.startSecond, endSecond: activityTrim.window.endSecond }
          : { startSecond: 0, endSecond: lane.durationSeconds }
      const textWidthPx = lane.durationSeconds > 0 ? (geometry.width * (textWindow.endSecond - textWindow.startSecond)) / lane.durationSeconds : 0
      const select = (event) => setSelectedClip({ laneId: lane.id, segmentKey: lane.isVideo ? segmentKeyFromEvent(event) : null })

      // Presentational lane model - TimelineLane renders these values without owning calculations.
      return {
        ...lane,
        segments: lane.isVideo
          ? segmentModels(
              lane.clip,
              selectedSegmentKey,
              clipFilmstripView(lane.clip, filmstrips[lane.clip.path], geometry, { viewStart, viewEnd, widthPx }),
            )
          : [],
        trimMasks: lane.isVideo || activityTrim === null ? [] : activityTrimMasks(activityTrim.window, lane.durationSeconds, viewStart, viewEnd),
        clipClassName: lane.isVideo ? (lane.warnings.length > 0 ? 'outline outline-1 -outline-offset-1 outline-destructive' : '') : 'bg-accent/70',
        clipContentClassName: CLIP_CONTENT_OFFSET_CLASS,
        clipProps: {
          ...dragProps,
          'aria-keyshortcuts': `ArrowLeft ArrowRight Shift+ArrowLeft Shift+ArrowRight${lane.isVideo ? ' Delete Backspace' : ''}`,
          'aria-pressed': isSelected,
          tabIndex: canSelect ? 0 : -1,
          onClick: (e) => {
            stopClipEvent(e)
            if (!canSelect) return
            // Pointer presses select on pointer down; a clip drag captures the pointer, so the click lands on the clip.
            if (e.detail !== 0) return
            e.currentTarget.focus()
            select(e)
          },
          onDoubleClick: stopClipEvent,
          onBlur: commitClipNudge,
          onKeyDown: (event) => {
            if (!isSelected || event.defaultPrevented) return
            const match = matchKeyboardShortcut(event, 'timeline-clip')
            if (match?.commandId === 'timelineClip.remove' && lane.isVideo && onRemoveVideo) {
              event.preventDefault()
              event.stopPropagation()
              onRemoveVideo(lane.clip.id)
              return
            }
            if (match?.commandId !== 'timelineClip.nudge') return
            event.preventDefault()
            event.stopPropagation()
            nudgeClip(lane.id, match.binding.seconds ?? CLIP_SYNC_STEP_SECONDS)
          },
          onKeyUp: (event) => {
            if (event.defaultPrevented) return
            const match = matchKeyboardShortcut(event, 'timeline-clip')
            if (match?.commandId !== 'timelineClip.nudge') return
            event.preventDefault()
            event.stopPropagation()
            commitClipNudge()
          },
          onMouseEnter: () => setHoveredLaneId(lane.id),
          onMouseLeave: () => setHoveredLaneId(null),
          onPointerDown: (event) => {
            if (canSelect && (event.button === undefined || event.button === 0)) {
              event.currentTarget.focus()
              select(event)
            }
            dragProps.onPointerDown?.(event)
          },
        },
        clipStyle: percentStyle(geometry, widthPx),
        textStyle:
          lane.durationSeconds > 0
            ? clipRangeStyle(textWindow.startSecond, textWindow.endSecond, lane.durationSeconds)
            : { left: '0%', width: '100%' },
        // A video clip's name sits in a badge at the left edge of the clip's visible part, over its filmstrip.
        badgeStyle: lane.isVideo ? { left: `${visibleStartPx - geometry.x}px`, maxWidth: `${visibleEndPx - visibleStartPx}px` } : null,
        durationLabel: formatClockDuration(lane.durationSeconds),
        isVisible: geometry.isVisible,
        // The whole clip shows as selected unless one of its segments is; then the segment alone carries the outline.
        isSelected: isSelected && selectedSegmentKey === null,
        hasSelectedSegment: isSelected && selectedSegmentKey !== null,
        showText: geometry.isVisible && textWidthPx >= TEXT_HIDE_THRESHOLD_REM * getRootRemPx(),
        sourceColumnWidth: CLIP_SOURCE_COLUMN_WIDTH,
        textClassName: 'text-accent-foreground',
        tooltip: {
          id: `${tooltipBaseId}-${lane.id}`,
          isVisible: showTooltip,
          style: {
            left: widthPx > 0 ? `${(tooltipAnchorPx / widthPx) * 100}%` : '50%',
          },
        },
      }
    })

    // Overlap bands - mark timeline intervals where clips overlap.
    const conflictBands = overlaps
      .map((overlap) => ({
        key: `${overlap.startSecond}:${overlap.endSecond}`,
        geometry: getClipGeometry({
          startSecond: overlap.startSecond,
          durationSeconds: overlap.endSecond - overlap.startSecond,
          viewStart,
          viewEnd,
          widthPx,
        }),
      }))
      .filter(({ geometry }) => geometry.isVisible)
      .map(({ key, geometry }) => ({ key, style: percentStyle(geometry, widthPx) }))

    // Join markers - one per join at the end of its outgoing piece, with the
    // transition window (the last `d` seconds of that piece) shaded.
    const piecesById = new Map((program?.pieces ?? []).map((piece) => [piece.id, piece]))
    // The join at the end of the segment before a dragged cut is hidden until the drop rebuilds the program.
    const draggedSegmentKey =
      draggedCut === null
        ? null
        : segmentBeforeCut(
            clips.find((clip) => clip.id === draggedCut.videoId),
            draggedCut.cutId,
          )
    const joinMarkers = (program?.joins ?? [])
      .filter((join) => {
        const from = piecesById.get(join.fromPieceId)
        return draggedCut === null || from.videoId !== draggedCut.videoId || from.segmentKey !== draggedSegmentKey
      })
      .map((join) => {
        const endSecond = piecesById.get(join.fromPieceId).timelineEndSecond
        const windowSeconds = join.transition.durationSeconds
        const band =
          windowSeconds > 0
            ? getClipGeometry({ startSecond: endSecond - windowSeconds, durationSeconds: windowSeconds, viewStart, viewEnd, widthPx })
            : null
        const control = joinModels.get(join.key)
        return {
          key: join.key,
          isVisible: endSecond >= viewStart && endSecond <= viewEnd,
          style: { left: widthPx > 0 ? `${(secondsToViewPx({ second: endSecond, viewStart, viewEnd, widthPx }) / widthPx) * 100}%` : '0%' },
          bandStyle: band?.isVisible ? percentStyle(band, widthPx) : null,
          isInvalid: control.error !== null,
          hasTransition: windowSeconds > 0,
          disabledReason: join.skippedDefault === null ? null : skippedDefaultLabel(join.skippedDefault, i18next.t),
          label: i18next.t('player.joinTransition', { defaultValue: 'Join: {{transition}}', transition: control.label }),
          control,
        }
      })
      .filter((marker) => marker.isVisible || marker.bandStyle !== null)
    // A join whose default transition did not fit is drawn outside the clip mask, so its tooltip is not clipped.
    const skippedJoinMarkers = joinMarkers.filter((marker) => marker.disabledReason !== null)

    // Cut markers - a hit area per cut of a clip, in the editing workspace: click to remove, drag to move. An edge
    // whose outer part is still exported is an edge cut: dragging it in trims that end. Clips too narrow on screen
    // have no edge cuts, so they can still be moved; a dragged edge cut is in the clip's cuts.
    const minEdgeCutClipPx = EDGE_CUT_MIN_CLIP_REM * getRootRemPx()
    const cutMarkers = (program === null || program === undefined ? [] : sortVideosByTimeline(clips))
      .flatMap((clip) => {
        const clipWidthPx = getClipGeometry({
          startSecond: clip.offsetSeconds,
          durationSeconds: clip.durationSeconds,
          viewStart,
          viewEnd,
          widthPx,
        }).width
        const cuts = [...clip.cuts, ...(clipWidthPx >= minEdgeCutClipPx ? listEdgeCuts(clip) : [])]
        // A fixed order (start edge, cuts, end edge) keeps a dragged edge cut's element in place, and its pointer capture.
        const bySide = (side) => cuts.filter((cut) => edgeCutSide(cut.id) === side)
        return [...bySide('start'), ...bySide(null), ...bySide('end')].map((cut) => {
          const sourceSecond = framesToSeconds(cut.frame, clip.fps)
          const second = clip.offsetSeconds + sourceSecond
          const side = edgeCutSide(cut.id)
          const name = filenameFromSelectedPath(clip.path)
          return {
            key: `${clip.id}:${cut.id}`,
            isVisible: second >= viewStart && second <= viewEnd,
            // An edge cut at its edge sits inside the clip; a dragged one is centred on its frame like any cut.
            align: cut.frame === 0 ? 'start' : cut.frame === clip.frameCount ? 'end' : 'center',
            style: { left: widthPx > 0 ? `${(secondsToViewPx({ second, viewStart, viewEnd, widthPx }) / widthPx) * 100}%` : '0%' },
            label:
              side === 'start'
                ? i18next.t('player.trimStart', { defaultValue: 'Drag to trim the start of {{name}}', name })
                : side === 'end'
                  ? i18next.t('player.trimEnd', { defaultValue: 'Drag to trim the end of {{name}}', name })
                  : i18next.t('player.cutAt', { defaultValue: 'Cut at {{time}}', time: formatClockDuration(sourceSecond) }),
            onRemove: side === null ? () => onRemoveCut(clip.id, cut.id) : null,
            isDragging: draggedCut?.videoId === clip.id && draggedCut.cutId === cut.id,
            dragProps: getCutDragProps(clip, cut),
          }
        })
      })
      .filter((marker) => marker.isVisible)

    // Activity trim handles - one per edge of the activity, inside the clip at its edge; dragging one in trims that end.
    const activityTrimMarkers =
      activityTrim === null || !hasActivity || activityGeometryWidthPx(activityDurationSeconds, viewStart, viewEnd, widthPx) < minEdgeCutClipPx
        ? []
        : ['start', 'end']
            .map((edge) => {
              const second = edge === 'start' ? activityTrim.window.startSecond : activityTrim.window.endSecond
              return {
                key: `activity:${edge}`,
                isVisible: second >= viewStart && second <= viewEnd,
                align: second <= 0 ? 'start' : second >= activityDurationSeconds ? 'end' : 'center',
                style: { left: widthPx > 0 ? `${(secondsToViewPx({ second, viewStart, viewEnd, widthPx }) / widthPx) * 100}%` : '0%' },
                label:
                  edge === 'start'
                    ? i18next.t('player.trimActivityStart', 'Drag to trim the start of the activity')
                    : i18next.t('player.trimActivityEnd', 'Drag to trim the end of the activity'),
                onRemove: null,
                isDragging: activityTrim.draggedEdge === edge,
                dragProps: activityTrim.getEdgeDragProps(edge),
              }
            })
            .filter((marker) => marker.isVisible)

    const videoClips = clipModels.filter((clip) => clip.isVideo)
    const activityClips = clipModels.filter((clip) => !clip.isVideo)
    return [
      ...(videoClips.length > 0
        ? [
            {
              id: 'video',
              ariaLabel: i18next.t('player.videoClipLane', 'Video clip lane'),
              isVideo: true,
              clips: videoClips,
              conflictBands,
              cutMarkers,
              joinMarkers: joinMarkers.filter((marker) => marker.disabledReason === null),
              skippedJoinMarkers,
            },
          ]
        : []),
      ...(activityClips.length > 0
        ? [
            {
              id: 'activity',
              ariaLabel: i18next.t('player.activityClipLane', 'Activity clip lane'),
              isVideo: false,
              clips: activityClips,
              conflictBands: [],
              cutMarkers: activityTrimMarkers,
              joinMarkers: [],
              skippedJoinMarkers: [],
            },
          ]
        : []),
    ]
  }, [
    activityFilename,
    activitySummary,
    activityTrim,
    canSelect,
    clips,
    commitClipNudge,
    draggedCut,
    filmstrips,
    getCutDragProps,
    getLaneDragProps,
    hasActivity,
    hoveredLaneId,
    joinModels,
    nudgeClip,
    onRemoveCut,
    onRemoveVideo,
    overlaps,
    program,
    selectedClip,
    setSelectedClip,
    stopClipEvent,
    tooltipBaseId,
    viewEnd,
    viewStart,
    warnings,
    widthPx,
  ])
}
