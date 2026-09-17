/**
 * Presentational telemetry graph and detected-event bands for the player timeline.
 */

import { useTranslation } from 'react-i18next'

function getBandClassName(tone) {
  return tone === 'stop' ? 'bg-video-sync-stop border-video-sync-stop/40' : 'bg-video-sync-turn border-video-sync-turn/40'
}

function getPathClassName(series) {
  return series === 'speed' ? 'text-video-sync-stop/40' : 'text-video-sync-turn/40'
}

/**
 * Renders the fixed-scale graph between the timeline ruler and lanes.
 *
 * @param {{ graph: object }} props Render-ready graph model.
 * @returns {JSX.Element} Graph presentation.
 */
export default function VideoSyncTimelineGraph({ graph }) {
  const { t } = useTranslation()
  const width = Math.max(1, graph.widthPx)

  return (
    <div
      aria-label={t('videoSync.timelineGraph', 'Activity telemetry graph')}
      className="relative h-16 w-full overflow-hidden border-x border-border/30 bg-background/20"
      data-testid="video-sync-timeline-graph"
      role="img"
    >
      <svg className="absolute inset-0 h-full w-full" viewBox={`0 0 ${width} ${graph.heightPx}`} preserveAspectRatio="none" aria-hidden="true">
        <path
          d={graph.paths.speed}
          fill="none"
          className={getPathClassName('speed')}
          stroke="currentColor"
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
        />
        <path
          d={graph.paths.turning}
          fill="none"
          className={getPathClassName('turning')}
          stroke="currentColor"
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
      {graph.eventBands.map((band) => (
        <div
          key={band.id}
          aria-label={band.ariaLabel}
          className={`pointer-events-none absolute bottom-0 top-0 border-x ${getBandClassName(band.tone)}`}
          style={band.style}
        >
          <span className="absolute left-0.5 top-0.5 whitespace-nowrap text-[0.55rem] font-bold uppercase leading-none text-foreground/80">
            {band.label}
          </span>
        </div>
      ))}
      <div className="pointer-events-none absolute bottom-0 left-1 flex gap-2 bg-background/50 px-1 py-0.5 text-[0.55rem] font-semibold uppercase leading-none">
        <span className="text-video-sync-stop">{t('videoSync.speed', 'Speed')}</span>
        <span className="text-video-sync-turn">{t('videoSync.turning', 'Turning')}</span>
      </div>
    </div>
  )
}
