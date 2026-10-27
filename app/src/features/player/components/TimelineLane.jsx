import { memo } from 'react'
import { useTranslation } from 'react-i18next'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { JoinTransitionControl } from '@/features/transitions'

const stopPointer = (event) => event.stopPropagation()

function JoinMarker({ marker }) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={marker.label}
          title={marker.label}
          className={`absolute top-1.5 z-30 h-3 w-3 -translate-x-1/2 rotate-45 rounded-[2px] border shadow-sm outline-none focus-visible:ring-2 focus-visible:ring-foreground ${
            marker.isInvalid
              ? 'border-destructive bg-destructive'
              : marker.hasTransition
                ? 'border-foreground/70 bg-primary'
                : 'border-foreground/60 bg-background'
          }`}
          style={marker.style}
          onPointerDown={stopPointer}
          onDoubleClick={stopPointer}
        />
      </PopoverTrigger>
      <PopoverContent side="top" className="w-72 space-y-2 p-3" onPointerDown={stopPointer}>
        <p className="text-xs font-semibold">{marker.label}</p>
        <JoinTransitionControl id={`timeline-${marker.key}`} model={marker.control} />
      </PopoverContent>
    </Popover>
  )
}

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
        {clip.cuts.map((cut) => (
          <div
            key={cut.key}
            data-testid="timeline-deleted-part"
            aria-hidden="true"
            className="pointer-events-none absolute inset-y-0 z-10 bg-background/75 bg-[repeating-linear-gradient(45deg,transparent_0,transparent_3px,rgb(255_255_255/0.12)_3px,rgb(255_255_255/0.12)_6px)]"
            style={cut.style}
          />
        ))}
        {clip.markStyle && (
          <div
            data-testid="timeline-deleted-part-mark"
            aria-hidden="true"
            className="pointer-events-none absolute -inset-y-1 z-10 w-0.5 -translate-x-1/2 bg-destructive"
            style={clip.markStyle}
          />
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
function TimelineLane({ lane }) {
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
        {lane.joinMarkers.map((marker) =>
          marker.bandStyle ? (
            <div
              key={`${marker.key}:band`}
              data-testid="timeline-transition-window"
              aria-hidden="true"
              className="pointer-events-none absolute top-3 bottom-1 z-20 bg-primary/35 bg-[linear-gradient(90deg,transparent,rgb(255_255_255/0.25))]"
              style={marker.bandStyle}
            />
          ) : null,
        )}
        {lane.joinMarkers.map((marker) => (marker.isVisible ? <JoinMarker key={marker.key} marker={marker} /> : null))}
      </div>
      {lane.clips.map((clip) => (clip.tooltip.isVisible && clip.label ? <TimelineClipTooltip key={clip.id} clip={clip} /> : null))}
    </div>
  )
}

export default memo(TimelineLane)
