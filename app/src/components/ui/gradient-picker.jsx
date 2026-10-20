/**
 * Provides a reusable linear gradient stop editor.
 */

import { Trash2Icon } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import HexColorPicker from '@/components/ui/hex-color-picker'
import { Label } from '@/components/ui/label'
import { useGradientPicker } from '@/hooks/useGradientPicker'
import { toLinearGradientCss } from '@/lib/gradient-stops'
import { cn } from '@/lib/utils'

/**
 * Renders a linear gradient bar. Clicking the bar adds a stop (up to five), stops drag
 * horizontally, clicking a stop opens the color picker, and Delete removes a stop.
 *
 * @param {object} props - Component props.
 * @param {{ offset: number, color: string }[]} props.stops - Gradient stops; offsets in [0, 1], colors as `#rrggbb`.
 * @param {(stops: { offset: number, color: string }[]) => void} props.onChange - Receives the next stops.
 * @param {string} [props.label] - Optional field label shown above the track.
 * @param {string} [props.className] - Additional class names for the root element.
 * @returns {JSX.Element} Rendered component output.
 */
export default function GradientPicker({ stops, onChange, label, className }) {
  const { t } = useTranslation()
  const { trackRef, canAddStop, canRemoveStop, handleTrackPointerDown, getStopHandlers, setStopColor, removeStop } = useGradientPicker({
    stops,
    onChange,
  })

  return (
    <div className={cn('space-y-3', className)}>
      {label ? <Label className="h-3 text-[9px] text-muted-foreground uppercase font-bold">{label}</Label> : null}
      {/* Matches the slider's vertical rhythm; the track's ::before extends its hit area into this padding. */}
      <div className="py-2">
        <div
          ref={trackRef}
          role="group"
          aria-label={label ?? t('components.gradientStops', 'Gradient stops')}
          title={canAddStop ? t('components.gradientAddStopHint', 'Click to add a color stop') : undefined}
          className={cn(
            'relative h-2.5 touch-none rounded-xs border border-border/70 shadow-xs before:absolute before:inset-x-0 before:-inset-y-2 before:content-[""]',
            canAddStop ? 'cursor-copy' : 'cursor-default',
          )}
          style={{ backgroundImage: toLinearGradientCss(stops) }}
          onPointerDown={handleTrackPointerDown}
        >
          {stops.map((stop, index) => (
            // Keying on the stop count remounts nodes when stops are added or removed,
            // so an open popover never moves to a different stop.
            <div
              key={`${stops.length}-${index}`}
              className="absolute top-1/2 -translate-x-1/2 -translate-y-1/2"
              style={{ left: `${stop.offset * 100}%` }}
            >
              <HexColorPicker
                value={stop.color}
                onChange={(color) => setStopColor(index, color)}
                trigger={
                  <button
                    type="button"
                    aria-label={t('components.gradientStopLabel', 'Color stop at {{percent}}%', { percent: Math.round(stop.offset * 100) })}
                    className="block size-3.5 cursor-grab rounded-full border-2 border-white shadow-[0_0_0_1px_rgb(0_0_0/0.5),0_1px_3px_rgb(0_0_0/0.4)] outline-none focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing data-[state=open]:scale-125"
                    style={{ backgroundColor: stop.color }}
                    {...getStopHandlers(index)}
                  />
                }
              >
                <div className="flex justify-end">
                  <Button
                    type="button"
                    variant="outline"
                    size="icon"
                    className="h-9 w-9 border-border/70 bg-surface text-muted-foreground hover:bg-surface-elevated hover:text-foreground"
                    aria-label={t('components.gradientRemoveStop', 'Remove stop')}
                    disabled={!canRemoveStop}
                    onClick={() => removeStop(index)}
                  >
                    <Trash2Icon />
                  </Button>
                </div>
              </HexColorPicker>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
