import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { GripVertical } from 'lucide-react'
import TimelineLane from './TimelineLane'
import { useTranslation } from 'react-i18next'

/**
 * Presentational timeline surface containing the axis, lanes, playhead, and export markers.
 *
 * @param {{ timeline: object }} props Timeline view model.
 */
export default function TimelineSurface({ timeline }) {
  const { t } = useTranslation()
  return (
    <div {...timeline.containerProps} className="relative mt-2" role="group" aria-label="Timeline">
      <div aria-label={t('player.timelineAxis', 'Timeline axis')} className="relative h-7 w-full cursor-crosshair select-none mt-4 mb-1" role="group" {...timeline.axisProps}>
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
        <div className="pointer-events-none absolute bottom-0 top-0 z-30 w-px -translate-x-1/2 bg-primary" style={timeline.playhead.lineStyle} />
        <div
          className="pointer-events-auto absolute -top-1 z-40 -translate-x-1/2 cursor-grab p-1 active:cursor-grabbing"
          style={timeline.playhead.style}
          {...timeline.playhead.handleProps}
        >
          <svg width="14" height="13" viewBox="0 0 14 13" className="fill-primary" aria-hidden="true">
            <polygon points="1,0 13,0 13,5 7,13 1,5" />
          </svg>
        </div>

        {timeline.exportMarkers.map((marker) => (
          <div key={marker.marker}>
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
        ))}
      </div>
    </div>
  )
}
