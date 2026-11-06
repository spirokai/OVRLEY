import { useTranslation } from 'react-i18next'
import { SecondsInput } from '@/components/ui/seconds-input'
import { TransitionSelect } from './TransitionSelect'

/**
 * Transition of one join: picker, the override's duration, and validation.
 *
 * @param {{model: object, id: string}} props Control model from `buildJoinTransitionModel` and an element ID prefix.
 */
export function JoinTransitionControl({ model, id }) {
  const { t } = useTranslation()
  return (
    <div className="space-y-1">
      <div className={`grid items-start gap-2 ${model.durationSeconds !== null ? 'grid-cols-[minmax(0,1fr)_6rem]' : 'grid-cols-1'}`}>
        <TransitionSelect
          id={id}
          value={model.value}
          label={model.label}
          previewType={model.previewType}
          defaultLabel={model.defaultLabel}
          onSelect={model.onSelect}
        />
        {model.durationSeconds !== null ? (
          <SecondsInput
            value={model.durationSeconds}
            fps={model.durationFps}
            mode="duration"
            onCommit={model.commitDuration}
            className="h-7 text-xs"
            aria-label={t('transitions.duration', 'Transition duration')}
          />
        ) : null}
      </div>
      {model.error !== null ? (
        <p role="alert" className="text-[0.65rem] text-destructive">
          {model.error}
        </p>
      ) : null}
    </div>
  )
}
