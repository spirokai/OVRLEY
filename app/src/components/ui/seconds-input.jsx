/**
 * The one input for times and durations in seconds: digits only, with the value
 * as a read-only clock readout inside the field (`330` → `05:30`).
 */

import { useId } from 'react'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { useSecondsInput } from '@/hooks/useSecondsInput'
import { cn } from '@/lib/utils'
import { useTranslation } from 'react-i18next'

// Width of one tabular readout character (0.65rem text) in rem; the field keeps that much room on its right.
const READOUT_CHARACTER_REM = 0.65 * 0.6

/**
 * Renders a seconds input. Typed text is kept until Enter or leaving the field,
 * then snapped to the frame grid and committed; Escape discards it.
 *
 * @param {object} props - Component props.
 * @param {number} props.value - Stored value in seconds.
 * @param {(seconds: number) => string|null|undefined} props.onCommit - Stores a snapped value; returns a message when it is rejected.
 * @param {{num: number, den: number}|null} props.fps - Frame grid the value snaps to, or null.
 * @param {'time'|'duration'} [props.mode='time'] - A duration is above 0 and at least one frame.
 * @param {boolean} [props.signed=false] - Whether negative values are allowed.
 * @param {number|null} [props.step=null] - Seconds the step buttons add or remove; null hides them.
 * @param {string} [props.className] - Classes of the input element.
 * @returns {JSX.Element} Rendered input.
 */
export function SecondsInput({ value, onCommit, fps, mode = 'time', signed = false, step = null, className, ...props }) {
  const { t } = useTranslation()
  const readoutId = useId()
  const field = useSecondsInput({ value, onCommit, fps, mode, signed, step })
  return (
    <div className="space-y-1">
      <div className="relative">
        <Input
          {...props}
          type="text"
          inputMode="decimal"
          value={field.text}
          onChange={field.onChange}
          onBlur={field.onBlur}
          onKeyDown={field.onKeyDown}
          aria-invalid={field.error !== null}
          aria-describedby={readoutId}
          className={cn('border-border/70 tabular-nums transition-colors', className)}
          style={{ paddingRight: `${field.readout.length * READOUT_CHARACTER_REM + (field.onStep === null ? 0.75 : 2.25)}rem` }}
        />
        <span
          id={readoutId}
          className={cn(
            'pointer-events-none absolute inset-y-0 flex items-center text-[0.65rem] text-muted-foreground tabular-nums',
            field.onStep === null ? 'right-2' : 'right-8',
          )}
        >
          {field.readout}
        </span>
        {field.onStep !== null ? (
          <div className="absolute inset-y-1 right-1 flex w-5 flex-col overflow-hidden rounded bg-surface-strong">
            <button
              type="button"
              className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => field.onStep(1)}
              disabled={props.disabled}
              aria-label={t('components.increaseValue', 'Increase value')}
            >
              <ChevronUp className="h-3 w-3" />
            </button>
            <div className="h-px bg-border/60" />
            <button
              type="button"
              className="flex flex-1 items-center justify-center text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => field.onStep(-1)}
              disabled={props.disabled}
              aria-label={t('components.decreaseValue', 'Decrease value')}
            >
              <ChevronDown className="h-3 w-3" />
            </button>
          </div>
        ) : null}
      </div>
      {field.error !== null ? (
        <p role="alert" className="text-[0.65rem] text-destructive">
          {field.error}
        </p>
      ) : null}
    </div>
  )
}
