/**
 * Presentational telemetry graph and detected-event bands for the player timeline.
 */

import { useTranslation } from 'react-i18next'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'

function getBandClassName(tone) {
  if (tone === 'stop') return 'bg-video-sync-stop/60'
  if (tone === 'location') return 'bg-video-sync-location/40'
  return 'bg-video-sync-turn/70'
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
      {graph.eventBands.map((band) => {
        const label = t(band.labelKey)
        const timing = {
          end: band.endSecond.toFixed(1),
          label,
          start: band.startSecond.toFixed(1),
        }

        return (
          <div key={band.id} className="pointer-events-auto absolute bottom-0 top-0" style={band.style}>
            <SimpleTooltip side="top" content={t('videoSync.detectedEvent', 'detected {{label}}', timing)} className="h-full w-full">
              <div
                aria-label={t('videoSync.detectedEventRange', '{{label}} event from {{start}} to {{end}} seconds', timing)}
                className={`h-full w-full border-none ${getBandClassName(band.tone)}`}
              />
            </SimpleTooltip>
          </div>
        )
      })}
      <div className="pointer-events-none absolute bottom-0 left-1 flex gap-2 bg-background/50 px-1 py-0.5 text-[0.60rem] font-semibold uppercase leading-none">
        <span className="text-video-sync-stop">{t('videoSync.speed', 'Speed')}</span>
        <span className="text-video-sync-turn">{t('videoSync.turning', 'Turning')}</span>
      </div>
    </div>
  )
}
