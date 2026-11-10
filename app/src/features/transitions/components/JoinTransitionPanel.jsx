import { useTranslation } from 'react-i18next'
import { SecondsInput } from '@/components/ui/seconds-input'
import { transitionPreviewStyle } from '../utils/transitionPreviews'
import { TransitionPicker } from './TransitionPicker'

/**
 * Transition of one join laid out inline: the active transition and its duration on top,
 * the transition options underneath.
 *
 * @param {{model: object}} props Control model from `buildJoinTransitionModel`.
 */
export function JoinTransitionPanel({ model }) {
  const { t } = useTranslation()
  return (
    <div className="space-y-2">
      <div className="flex h-7 items-center gap-2">
        <span className="flex min-w-0 flex-1 items-center gap-2 text-xs">
          {model.previewType !== null ? (
            <span aria-hidden="true" style={transitionPreviewStyle(model.previewType)} className="h-4 w-7 shrink-0 rounded-[2px]" />
          ) : null}
          <span className="truncate py-1 font-medium [text-box:trim-both_cap_alphabetic]">{model.label}</span>
        </span>
        {model.durationSeconds !== null ? (
          <SecondsInput
            value={model.durationSeconds}
            fps={model.durationFps}
            mode="duration"
            onCommit={model.commitDuration}
            className="h-7 w-24 text-xs"
            aria-label={t('transitions.duration', 'Transition duration')}
          />
        ) : null}
      </div>
      {model.error !== null ? (
        <p role="alert" className="text-[0.65rem] text-destructive">
          {model.error}
        </p>
      ) : null}
      <div className="border-t border-border/40 pt-2">
        <TransitionPicker value={model.value} defaultLabel={model.defaultLabel} onSelect={model.onSelect} />
      </div>
    </div>
  )
}
