/**
 * Presentational telemetry graph and detected-event bands for the player timeline.
 */

import { useTranslation } from 'react-i18next'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'

function getBandClassName(tone) {
  return tone === 'stop' ? 'bg-video-sync-stop/80' : 'bg-video-sync-turn/60'
}

function getPathClassName(series) {
  return series === 'speed' ? 'text-video-sync-stop/40' : 'text-video-sync-turn/30'
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
      className="relative h-16 w-full overflow-visible border-x border-border/30 bg-background/20 mb-1 mt-3"
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
        <div key={band.id} className="pointer-events-auto absolute bottom-0 top-0" style={band.style}>
          <SimpleTooltip side="top" content={`detected ${band.label}`} className="h-full w-full">
            <div aria-label={band.ariaLabel} className={`h-full w-full border-none ${getBandClassName(band.tone)}`} />
          </SimpleTooltip>
        </div>
      ))}
      <div className="pointer-events-none absolute bottom-0 left-1 flex gap-2 bg-background/50 px-1 py-0.5 text-[0.60rem] font-semibold uppercase leading-none">
        <span className="text-video-sync-stop">{t('videoSync.speed', 'Speed')}</span>
        <span className="text-video-sync-turn">{t('videoSync.turning', 'Turning')}</span>
      </div>
    </div>
  )
}
