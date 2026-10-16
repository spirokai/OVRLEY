import { useTranslation } from 'react-i18next'

function TimelineClip({ clip }) {
  const Icon = clip.icon
  return (
    <button
      type="button"
      aria-describedby={clip.tooltip.isVisible ? clip.tooltip.id : undefined}
      aria-label={clip.label || 'clip'}
      className="group absolute top-3 bottom-1 z-10 cursor-grab touch-none appearance-none border-0 bg-transparent p-0 text-left outline-none active:cursor-grabbing"
      style={clip.clipStyle}
      {...clip.clipProps}
    >
      <div
        className={`absolute inset-0 overflow-visible group-focus-visible:ring-2 group-focus-visible:ring-foreground ${clip.isSelected ? 'ring-2 ring-foreground' : ''} ${clip.clipClassName}`}
      >
        {clip.highlightStyle && (
          <div aria-hidden="true" className="pointer-events-none absolute -top-3 -bottom-1 bg-success/20" style={clip.highlightStyle} />
        )}
        {clip.showText && (
          <div
            className={`relative grid h-full items-center gap-3 overflow-hidden whitespace-nowrap px-2.5 text-[0.7rem] font-bold uppercase leading-none ${clip.textClassName}`}
            style={{ gridTemplateColumns: `${clip.sourceColumnWidth} minmax(0, 1fr) auto` }}
          >
            <span className={`flex min-w-0 items-center justify-left pl-2 ${clip.clipContentClassName}`}>
              {Icon ? (
                <Icon className="h-5 w-5 shrink-0 pb-0.5" strokeWidth={3} aria-hidden="true" />
              ) : (
                <span className="block max-w-full truncate text-[0.85rem] font-black leading-none">{clip.formatLabel}</span>
              )}
            </span>
            <span className={`min-w-0 truncate leading-none ${clip.clipContentClassName}`}>{clip.label}</span>
            <span className={`shrink-0 pl-1 tabular-nums leading-none ${clip.clipContentClassName}`}>{clip.durationLabel}</span>
          </div>
        )}
      </div>
    </button>
  )
}

function TimelineClipTooltip({ clip }) {
  const { t } = useTranslation()
  return (
    <div
      id={clip.tooltip.id}
      role="tooltip"
      className="pointer-events-none absolute bottom-full z-1000 mb-2 max-w-124 -translate-x-1/2 rounded border border-border/70 bg-surface-tooltip px-2.5 py-1.5 text-left text-xs text-foreground shadow-2xl"
      style={clip.tooltip.style}
    >
      <div className="text-[0.72rem] font-semibold leading-snug">{clip.label}</div>
      <div className="mt-1 flex items-center justify-between gap-4 border-t border-border/40 pt-1 text-[0.65rem] text-muted-foreground">
        <span className="font-medium">{t('player.duration', 'Duration')}</span>
        <span className="tabular-nums text-foreground">{clip.durationLabel}</span>
      </div>
      {clip.availableMetrics?.length > 0 && (
        <p className="mt-1 border-t border-border/40 pt-1 text-[0.6rem] leading-relaxed text-foreground font-medium">
          {clip.availableMetrics.join(', ')}
        </p>
      )}
      {clip.warnings.length > 0 && (
        <ul className="mt-1 border-t border-border/40 pt-1 text-[0.65rem] leading-relaxed text-destructive">
          {clip.warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      )}
      <div className="absolute left-1/2 top-full -mt-px -translate-x-1/2 border-4 border-transparent border-t-surface-tooltip" />
    </div>
  )
}

/**
 * Presentational lane renderer: the lane's clips, overlap bands, and clip tooltips.
 *
 * @param {{ lane: object }} props Lane view model.
 */
export default function TimelineLane({ lane }) {
  const { t } = useTranslation()
  return (
    <div aria-label={lane.ariaLabel} className={`relative w-full ${lane.isVideo ? 'h-6 border-b-0' : 'h-6'}`}>
      <div data-testid="timeline-lane-clip-mask" className="absolute inset-x-0 -top-3 -bottom-1 overflow-hidden">
        {lane.clips.map((clip) => (clip.isVisible ? <TimelineClip key={clip.id} clip={clip} /> : null))}
        {lane.conflictBands.map((band) => (
          <div
            key={band.key}
            aria-label={t('player.overlapBand', 'Overlapping videos')}
            className="pointer-events-none absolute top-3 bottom-1 z-20 bg-destructive/45 bg-[repeating-linear-gradient(135deg,transparent_0,transparent_4px,rgb(0_0_0/0.25)_4px,rgb(0_0_0/0.25)_8px)]"
            role="img"
            style={band.style}
          />
        ))}
      </div>
      {lane.clips.map((clip) => (clip.tooltip.isVisible && clip.label ? <TimelineClipTooltip key={clip.id} clip={clip} /> : null))}
    </div>
  )
}
