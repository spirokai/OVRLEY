import { memo } from 'react'
import { GripVertical } from 'lucide-react'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import VideoSyncTimelineGraph from '@/features/video-sync/components/VideoSyncTimelineGraph'
import VideoSyncTimelineLandmarks from '@/features/video-sync/components/VideoSyncTimelineLandmarks'
import TimelineLane from './TimelineLane'
import { useTranslation } from 'react-i18next'
import { PLAYHEAD_HEAD_HEIGHT, PLAYHEAD_HEAD_SHOULDER, PLAYHEAD_HIT_PADDING, PLAYHEAD_LINE_X, PLAYHEAD_WIDTH } from '../data/playheadGeometry'

const PLAYHEAD_HEAD_POINTS = `0,0 ${PLAYHEAD_WIDTH},0 ${PLAYHEAD_WIDTH},${PLAYHEAD_HEAD_SHOULDER} ${PLAYHEAD_WIDTH / 2},${PLAYHEAD_HEAD_HEIGHT} 0,${PLAYHEAD_HEAD_SHOULDER}`

/**
 * Export range marker handle and guide line; memoized so playhead frames skip its tooltip tree.
 *
 * @param {{ marker: object }} props Export marker view model.
 */
const TimelineExportMarker = memo(function TimelineExportMarker({ marker }) {
  return (
    <div>
      <div className="pointer-events-none absolute bottom-0 top-4 w-px -translate-x-1/2 bg-success" style={marker.lineStyle} />
      <div className="pointer-events-auto absolute -top-1 z-20 -translate-x-1/2" style={marker.style}>
        <SimpleTooltip side="top" content={marker.label}>
          <button
            type="button"
            aria-label={marker.label}
            className="relative flex h-6 w-5 cursor-ew-resize items-start justify-center bg-transparent p-0 text-success outline-none focus-visible:ring-2 focus-visible:ring-success/50 active:cursor-ew-resize"
            {...marker.markerProps}
          >
            <svg width="14" height="20" viewBox="0 0 14 20" className="mt-1 fill-current" aria-hidden="true">
              <rect x="2.5" y="2" width="9" height="16" rx="1.5" />
            </svg>
            <GripVertical
              aria-hidden="true"
              className="pointer-events-none absolute left-1/2 top-4 h-3 w-3 -translate-x-1/2 -translate-y-1/2 text-background/80"
              strokeWidth={2.5}
            />
          </button>
        </SimpleTooltip>
      </div>
    </div>
  )
})

/**
 * Presentational timeline surface containing the axis, lanes, playhead, and export markers.
 *
 * @param {{ timeline: object }} props Timeline view model.
 */
export default function TimelineSurface({ timeline }) {
  const { t } = useTranslation()
  return (
    <div {...timeline.containerProps} className="relative mt-2" role="group" aria-label="Timeline">
      <div
        aria-label={t('player.timelineAxis', 'Timeline axis')}
        className="relative h-7 w-full cursor-crosshair select-none mt-4 mb-1"
        role="group"
        {...timeline.axisProps}
      >
        <div className="pointer-events-none absolute inset-0 overflow-hidden">
          <div className="absolute inset-0 will-change-transform" style={timeline.ticks.stripStyle}>
            {timeline.ticks.minor.map((tick) => (
              <div key={tick.id} className="absolute top-0 h-1.5 w-px bg-border/70" style={tick.lineStyle} />
            ))}
            {timeline.ticks.major.map((tick) => (
              <div key={tick.id}>
                <div className="absolute top-0 h-2.5 w-px bg-border" style={tick.lineStyle} />
                <span className="absolute top-3 -translate-x-1/2 text-[0.6rem] tabular-nums text-muted-foreground" style={tick.labelStyle}>
                  {tick.label}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>

      {timeline.videoSyncMode && timeline.graph ? <VideoSyncTimelineGraph graph={timeline.graph} /> : null}

      <div
        aria-label={t('player.timelineLaneBackground', 'Timeline lane background')}
        className="relative w-full cursor-e-resize select-none bg-foreground/10 active:cursor-e-resize border border-border/40 space-y-0.5 py-1"
        role="group"
        {...timeline.panSurfaceProps}
      >
        {timeline.snapGuidelineStyle && (
          <div
            aria-hidden="true"
            className="pointer-events-none absolute bottom-0 top-0 z-20 w-px bg-success shadow-[0_0_6px_hsl(var(--success))]"
            style={timeline.snapGuidelineStyle}
          />
        )}
        {timeline.lanes.map((lane) => (
          <TimelineLane key={lane.id} lane={lane} />
        ))}
      </div>

      <div className="pointer-events-none absolute inset-0">
        {timeline.videoSyncMode ? <VideoSyncTimelineLandmarks landmarks={timeline.landmarks} /> : null}
        {/* One SVG for head and line, so the line can never drift against the head. */}
        <svg
          data-testid="timeline-playhead"
          width={PLAYHEAD_WIDTH}
          aria-hidden="true"
          className="pointer-events-none absolute bottom-0 left-0 top-0 z-40 h-full overflow-visible fill-primary"
          style={timeline.playhead.style}
        >
          <rect x={PLAYHEAD_LINE_X} y="0" width="1" height="100%" />
          <polygon points={PLAYHEAD_HEAD_POINTS} />
          <rect
            x={-PLAYHEAD_HIT_PADDING}
            y={-PLAYHEAD_HIT_PADDING}
            width={PLAYHEAD_WIDTH + 2 * PLAYHEAD_HIT_PADDING}
            height={PLAYHEAD_HEAD_HEIGHT + 2 * PLAYHEAD_HIT_PADDING}
            fill="transparent"
            className="pointer-events-auto cursor-grab active:cursor-grabbing"
            {...timeline.playhead.handleProps}
          />
        </svg>

        {timeline.exportMarkers.map((marker) => (
          <TimelineExportMarker key={marker.marker} marker={marker} />
        ))}
      </div>
    </div>
  )
}
