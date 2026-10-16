/**
 * Builds render-ready lane models: one video lane holding every clip with its
 * overlap bands, and the activity lane.
 */

import { useCallback, useId, useMemo, useState } from 'react'
import { Video } from 'lucide-react'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { formatClockDuration } from '@/lib/time-format'
import { getClipGeometry, getExportRangeHighlightGeometry, secondsToViewPx } from '../utils/timelineGeometry'
import { getActivityAttributeLabel } from '@/lib/widget/widget-presentation'
import { sortVideosByTimeline } from '@/lib/video-program/video-collection'
import i18next from 'i18next'

const TEXT_HIDE_THRESHOLD_REM = 3
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

function getBasename(path) {
  return path.split(/[\\/]/).pop()
}

function videoIdFromClipId(clipId) {
  return clipId.slice('video:'.length)
}

function clipRangeStyle(fromSecond, toSecond, durationSeconds) {
  return { left: `${(fromSecond / durationSeconds) * 100}%`, width: `${((toSecond - fromSecond) / durationSeconds) * 100}%` }
}

/** Deleted parts, the drawn range and the marked start, positioned within their clip. */
function deletedPartOverlays(clip, dragPreview, mark) {
  const cuts = clip.deletedParts.map((part) => ({ key: part.id, style: clipRangeStyle(part.fromSecond, part.toSecond, clip.durationSeconds) }))
  if (dragPreview?.videoId === clip.id) {
    cuts.push({ key: 'drawing', style: clipRangeStyle(dragPreview.fromSecond, dragPreview.toSecond, clip.durationSeconds) })
  }
  return {
    cuts,
    markStyle: mark?.videoId === clip.id ? { left: `${(mark.sourceSecond / clip.durationSeconds) * 100}%` } : null,
  }
}

function percentStyle(geometry, widthPx) {
  return {
    left: widthPx > 0 ? `${(geometry.x / widthPx) * 100}%` : '0%',
    width: widthPx > 0 ? `${(geometry.width / widthPx) * 100}%` : '0%',
  }
}

/**
 * Owns lane and clip view models, tooltip state, clip geometry, overlap bands, and export highlights.
 *
 * @param {object} options Clip timeline inputs.
 * @param {string|null} options.activityFilename Imported activity filename.
 * @param {object|null} options.activitySummary External activity summary metadata.
 * @param {boolean} options.canSelect Whether clips can be selected for keyboard nudges.
 * @param {Array<object>} options.clips Displayed timeline clips.
 * @param {function} options.commitClipNudge Callback to commit a keyboard sync adjustment.
 * @param {object} options.deletedPartDrag Alt+drag handlers and preview from useDeletedPartDrag.
 * @param {{videoId: string, sourceSecond: number}|null} options.deletedPartMark Marked start of a deleted part.
 * @param {{ fromSecond: number, toSecond: number }|null} options.exportHighlightRange Active export highlight range.
 * @param {function} options.getLaneDragProps Per-lane drag handler factory from useClipDrag.
 * @param {boolean} options.hasActivity Whether the external activity lane should be present.
 * @param {Map<string, object>} [options.joinModels] Transition controls keyed by join key (`useJoinTransitionModels`).
 * @param {function} options.nudgeClip Callback to move a clip by a relative number of seconds.
 * @param {function} [options.onRemoveVideo] Callback to remove a video clip by video ID.
 * @param {Array<{startSecond: number, endSecond: number}>} [options.overlaps] Overlapping timeline intervals of the displayed clips.
 * @param {{pieces: Array<object>, joins: Array<object>}} [options.program] Displayed program, for its join markers.
 * @param {string|null} options.selectedClipId Currently selected clip id.
 * @param {function} options.setSelectedClipId Callback to set selected clip id.
 * @param {number} options.viewEnd Visible viewport end second.
 * @param {number} options.viewStart Visible viewport start second.
 * @param {Map<string, Array<{message: string}>>} [options.warnings] Clip warnings keyed by video ID.
 * @param {number} options.widthPx Measured timeline width.
 * @returns {Array<object>} Presentational lane view models, each with its clips.
 */
export default function useTimelineClips({
  activityFilename,
  activitySummary,
  canSelect,
  clips,
  commitClipNudge,
  deletedPartDrag,
  deletedPartMark,
  exportHighlightRange,
  getLaneDragProps,
  hasActivity,
  joinModels,
  nudgeClip,
  onRemoveVideo,
  overlaps = [],
  program,
  selectedClipId,
  setSelectedClipId,
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
        icon: Video,
        id: `video:${clip.id}`,
        isVideo: true,
        label: getBasename(clip.path),
        startSecond: clip.offsetSeconds,
        warnings: (warnings?.get(clip.id) ?? []).map((warning) => warning.message),
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
        id: 'activity',
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
      const highlight = exportHighlightRange
        ? getExportRangeHighlightGeometry({
            startSecond: lane.startSecond,
            durationSeconds: lane.durationSeconds,
            exportFromSecond: exportHighlightRange.fromSecond,
            exportToSecond: exportHighlightRange.toSecond,
          })
        : null

      // Tooltip position - anchor to the visible portion of partially clipped lanes.
      const visibleStartPx = geometry.isVisible ? clampPx(geometry.x, 0, widthPx) : 0
      const visibleEndPx = geometry.isVisible ? clampPx(geometry.x + geometry.width, 0, widthPx) : 0
      const tooltipAnchorPx = visibleStartPx + Math.max(0, visibleEndPx - visibleStartPx) / 2
      const showTooltip = hoveredLaneId === lane.id && Boolean(lane.label)

      // Presentational lane model - TimelineLane renders these values without owning calculations.
      return {
        ...lane,
        ...(lane.isVideo ? deletedPartOverlays(lane.clip, deletedPartDrag.preview, deletedPartMark) : { cuts: [], markStyle: null }),
        clipClassName: lane.isVideo
          ? `bg-accent/70 ${lane.warnings.length > 0 ? 'outline outline-1 -outline-offset-1 outline-destructive' : ''}`
          : 'bg-primary/80',
        clipContentClassName: CLIP_CONTENT_OFFSET_CLASS,
        clipProps: {
          ...dragProps,
          'aria-keyshortcuts': `ArrowLeft ArrowRight Shift+ArrowLeft Shift+ArrowRight${lane.isVideo ? ' Delete Backspace' : ''}`,
          'aria-pressed': selectedClipId === lane.id,
          tabIndex: canSelect ? 0 : -1,
          onClick: (e) => {
            stopClipEvent(e)
            if (!canSelect) return
            e.currentTarget.focus()
            setSelectedClipId(lane.id)
          },
          onDoubleClick: stopClipEvent,
          onBlur: commitClipNudge,
          onKeyDown: (event) => {
            if (selectedClipId !== lane.id || event.defaultPrevented) return
            const match = matchKeyboardShortcut(event, 'timeline-clip')
            if (match?.commandId === 'timelineClip.remove' && lane.isVideo && onRemoveVideo) {
              event.preventDefault()
              event.stopPropagation()
              onRemoveVideo(videoIdFromClipId(lane.id))
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
              setSelectedClipId(lane.id)
            }
            if (lane.isVideo && deletedPartDrag.onPointerDown(event, lane.clip)) return
            dragProps.onPointerDown?.(event)
          },
          onPointerMove: (event) => {
            if (!deletedPartDrag.onPointerMove(event)) dragProps.onPointerMove?.(event)
          },
          onPointerUp: (event) => {
            if (!deletedPartDrag.onPointerUp(event)) dragProps.onPointerUp?.(event)
          },
          onPointerCancel: (event) => {
            if (!deletedPartDrag.onPointerCancel(event)) dragProps.onPointerCancel?.(event)
          },
        },
        clipStyle: percentStyle(geometry, widthPx),
        durationLabel: formatClockDuration(lane.durationSeconds),
        highlightStyle:
          highlight?.isVisible === true
            ? {
                left: `${highlight.leftPercent}%`,
                width: `${highlight.widthPercent}%`,
              }
            : null,
        isVisible: geometry.isVisible,
        isSelected: selectedClipId === lane.id,
        showText: geometry.isVisible && geometry.width >= TEXT_HIDE_THRESHOLD_REM * getRootRemPx(),
        sourceColumnWidth: CLIP_SOURCE_COLUMN_WIDTH,
        textClassName: lane.isVideo ? 'text-accent-foreground' : 'text-background',
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
    const joinMarkers = (program?.joins ?? [])
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
          label: i18next.t('player.joinTransition', { defaultValue: 'Join: {{transition}}', transition: control.label }),
          control,
        }
      })
      .filter((marker) => marker.isVisible || marker.bandStyle !== null)

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
              joinMarkers,
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
              joinMarkers: [],
            },
          ]
        : []),
    ]
  }, [
    activityFilename,
    activitySummary,
    canSelect,
    clips,
    commitClipNudge,
    deletedPartDrag,
    deletedPartMark,
    exportHighlightRange,
    getLaneDragProps,
    hasActivity,
    hoveredLaneId,
    joinModels,
    nudgeClip,
    onRemoveVideo,
    overlaps,
    program,
    selectedClipId,
    setSelectedClipId,
    stopClipEvent,
    tooltipBaseId,
    viewEnd,
    viewStart,
    warnings,
    widthPx,
  ])
}
