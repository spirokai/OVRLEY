import { useTranslation } from 'react-i18next'
/**
 * Presentational clip lane renderer.
 *
 * @param {{ lane: object }} props Lane view model.
 */
export default function TimelineLane({ lane }) {
  const { t } = useTranslation()
  const Icon = lane.icon

  return (
    <div aria-label={lane.ariaLabel} className={`relative w-full ${lane.isVideo ? 'h-6 border-b-0' : 'h-6'}`}>
      <div data-testid="timeline-lane-clip-mask" className="absolute inset-x-0 -top-3 -bottom-1 overflow-hidden">
        {lane.isVisible && (
          <button
            type="button"
            aria-describedby={lane.tooltip.isVisible ? lane.tooltip.id : undefined}
            aria-label={lane.label || 'clip'}
            className="group absolute top-3 bottom-1 z-10 cursor-grab touch-none appearance-none border-0 bg-transparent p-0 text-left outline-none active:cursor-grabbing"
            style={lane.clipStyle}
            {...lane.clipProps}
          >
            <div
              className={`absolute inset-0 overflow-visible group-focus-visible:ring-2 group-focus-visible:ring-foreground ${lane.isSelected ? 'ring-2 ring-foreground' : ''} ${lane.clipClassName}`}
            >
              {lane.highlightStyle && (
                <div aria-hidden="true" className="pointer-events-none absolute -top-3 -bottom-1 bg-success/20" style={lane.highlightStyle} />
              )}
              {lane.showText && (
                <div
                  className={`relative grid h-full items-center gap-3 overflow-hidden whitespace-nowrap px-2.5 text-[0.7rem] font-bold uppercase leading-none ${lane.textClassName}`}
                  style={{ gridTemplateColumns: `${lane.sourceColumnWidth} minmax(0, 1fr) auto` }}
                >
                  <span className={`flex min-w-0 items-center justify-left pl-2 ${lane.clipContentClassName}`}>
                    {Icon ? (
                      <Icon className="h-5 w-5 shrink-0 pb-0.5" strokeWidth={3} aria-hidden="true" />
                    ) : (
                      <span className="block max-w-full truncate text-[0.85rem] font-black leading-none">{lane.formatLabel}</span>
                    )}
                  </span>
                  <span className={`min-w-0 truncate leading-none ${lane.clipContentClassName}`}>{lane.label}</span>
                  <span className={`shrink-0 pl-1 tabular-nums leading-none ${lane.clipContentClassName}`}>{lane.durationLabel}</span>
                </div>
              )}
            </div>
          </button>
        )}
      </div>
      {lane.tooltip.isVisible && lane.label && (
        <div
          id={lane.tooltip.id}
          role="tooltip"
          className="pointer-events-none absolute bottom-full z-1000 mb-2 max-w-124 -translate-x-1/2 rounded border border-border/70 bg-surface-tooltip px-2.5 py-1.5 text-left text-xs text-foreground shadow-2xl"
          style={lane.tooltip.style}
        >
          <div className="text-[0.72rem] font-semibold leading-snug">{lane.label}</div>
          <div className="mt-1 flex items-center justify-between gap-4 border-t border-border/40 pt-1 text-[0.65rem] text-muted-foreground">
            <span className="font-medium">{t('player.duration', 'Duration')}</span>
            <span className="tabular-nums text-foreground">{lane.durationLabel}</span>
          </div>
          {lane.availableMetrics?.length > 0 && (
            <p className="mt-1 border-t border-border/40 pt-1 text-[0.6rem] leading-relaxed text-foreground font-medium">
              {lane.availableMetrics.join(', ')}
            </p>
          )}
          <div className="absolute left-1/2 top-full -mt-px -translate-x-1/2 border-4 border-transparent border-t-surface-tooltip" />
        </div>
      )}
    </div>
  )
}
