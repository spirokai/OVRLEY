import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { BlurInput } from '@/components/ui/blur-input'
import { TransitionSelect } from './TransitionSelect'

/**
 * Transition of one join: picker, the override's duration, and validation.
 *
 * @param {{model: object, id: string}} props Control model from `buildJoinTransitionModel` and an element ID prefix.
 */
export function JoinTransitionControl({ model, id }) {
  const { t } = useTranslation()
  const [durationError, setDurationError] = useState(null)
  const error = durationError ?? model.error
  return (
    <div className="space-y-1">
      <div className="grid grid-cols-[minmax(0,1fr)_4rem] items-center gap-2">
        <TransitionSelect
          id={id}
          value={model.value}
          label={model.label}
          previewType={model.previewType}
          defaultLabel={model.defaultLabel}
          disabled={!model.isAvailable}
          onSelect={(value) => {
            setDurationError(null)
            model.onSelect(value)
          }}
        />
        {model.durationInput !== null ? (
          <BlurInput
            type="text"
            inputMode="decimal"
            value={model.durationInput}
            onChange={(event) => setDurationError(model.commitDuration(event.target.value))}
            className="h-7 text-xs"
            aria-label={t('transitions.duration', 'Transition duration (seconds)')}
            aria-invalid={durationError !== null}
          />
        ) : null}
      </div>
      {error !== null ? (
        <p role="alert" className="text-[0.65rem] text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  )
}
