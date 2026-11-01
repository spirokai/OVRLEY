/**
 * The one input for times and durations: the value is typed as a clock (`05:30`),
 * as plain seconds, or as a sum of both (`05:30+10`). While a complete draft is
 * typed, a preview below the field reads it as a clock and in seconds.
 */

import { useId } from 'react'
import { ChevronDown, ChevronUp, Clock, RotateCcw } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { useSecondsInput } from '@/hooks/useSecondsInput'
import { cn } from '@/lib/utils'
import { useTranslation } from 'react-i18next'

/**
 * Renders a seconds input. Typed text is kept until Enter or leaving the field,
 * then snapped to the frame grid and committed; Escape discards it. Up and Down
 * move the value by 1 s, 10 s with Shift, or one frame with Alt.
 *
 * @param {object} props - Component props.
 * @param {number} props.value - Stored value in seconds.
 * @param {(seconds: number) => string|null|undefined} props.onCommit - Stores a snapped value; returns a message when it is rejected.
 * @param {{num: number, den: number}|null} props.fps - Frame grid the value snaps to, or null.
 * @param {'time'|'duration'} [props.mode='time'] - A duration is above 0 and at least one frame.
 * @param {boolean} [props.signed=false] - Whether negative values are allowed.
 * @param {number|null} [props.step=null] - Seconds the step buttons add or remove; null hides them.
 * @param {{isManual: boolean, onReset: function, label: string, manualHint: string, autoHint: string}|null} [props.reset=null] - Placement
 *   slot inside the field: a reset button for a value set by hand, a clock for an automatic one; null hides it.
 * @param {string} [props.className] - Classes of the input element.
 * @returns {JSX.Element} Rendered input.
 */
export function SecondsInput({ value, onCommit, fps, mode = 'time', signed = false, step = null, reset = null, className, ...props }) {
  const { t } = useTranslation()
  const messageId = useId()
  const field = useSecondsInput({ value, onCommit, fps, mode, signed, step, onReset: reset !== null && reset.isManual ? reset.onReset : null })
  const slotPosition = field.onStep === null ? 'right-1' : 'right-7'
  return (
    <div className="space-y-1">
      <div className="relative">
        <Input
          {...props}
          type="text"
          value={field.text}
          onChange={field.onChange}
          onBlur={field.onBlur}
          onKeyDown={field.onKeyDown}
          aria-invalid={field.error !== null}
          aria-describedby={field.error !== null || field.preview !== null ? messageId : undefined}
          className={cn(
            'border-border/70 tabular-nums transition-colors',
            className,
            field.onStep !== null && reset !== null ? 'pr-13' : (field.onStep !== null || reset !== null) && 'pr-7',
          )}
        />
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
        {field.onReset !== null ? (
          <button
            type="button"
            className={cn(
              'absolute inset-y-1 flex w-5 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-surface-accent-soft hover:text-primary disabled:pointer-events-none disabled:opacity-50',
              slotPosition,
            )}
            onMouseDown={(event) => event.preventDefault()}
            onClick={field.onReset}
            disabled={props.disabled}
            aria-label={reset.label}
            title={reset.manualHint}
          >
            <RotateCcw className="h-3 w-3" />
          </button>
        ) : reset !== null ? (
          <span
            role="img"
            className={cn(
              'absolute inset-y-1 flex w-5 items-center justify-center text-muted-foreground/50',
              slotPosition,
              props.disabled && 'opacity-50',
            )}
            aria-label={reset.autoHint}
            title={reset.autoHint}
          >
            <Clock className="h-3 w-3" />
          </span>
        ) : null}
      </div>
      {field.error !== null ? (
        <p id={messageId} role="alert" className="text-[0.65rem] text-destructive">
          {field.error}
        </p>
      ) : field.preview !== null ? (
        <p id={messageId} className="text-[0.65rem] text-muted-foreground tabular-nums">
          {field.preview}
        </p>
      ) : null}
    </div>
  )
}
