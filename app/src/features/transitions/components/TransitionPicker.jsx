import { useState } from 'react'
import { Ban } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import { NO_TRANSITION, XFADE_TRANSITIONS } from '@/lib/video-program/transition-catalog'
import { DEFAULT_TRANSITION_OPTION } from '../utils/joinTransitionModel'
import { transitionPreviewStyle } from '../utils/transitionPreviews'

function PickerTile({ isSelected, label, onSelect, children }) {
  return (
    <button
      type="button"
      role="option"
      aria-label={label}
      aria-selected={isSelected}
      className={cn(
        'flex min-w-0 flex-col gap-1 rounded-xs border p-1 text-left text-[0.65rem] hover:border-primary focus-visible:outline-2 focus-visible:outline-primary',
        isSelected ? 'border-primary bg-surface-accent-soft' : 'border-border/60',
      )}
      onClick={onSelect}
    >
      <div className="flex aspect-[120/68] w-full items-center justify-center overflow-hidden rounded-[2px] bg-muted">{children}</div>
      <span className="truncate font-medium">{label}</span>
    </button>
  )
}

/**
 * Filterable grid of transition options: an animated preview per `xfade` transition, no
 * transition, and (for joins) the project default.
 *
 * @param {object} props Picker props.
 * @param {string} props.value Selected option: an `xfade` name, `none`, or `default`.
 * @param {string|null} [props.defaultLabel] Label of the default option; omitted for the project default itself.
 * @param {(value: string) => void} props.onSelect Called with the chosen option.
 */
export function TransitionPicker({ value, defaultLabel = null, onSelect }) {
  const { t } = useTranslation()
  const [filter, setFilter] = useState('')
  const types = XFADE_TRANSITIONS.filter((type) => type.includes(filter.trim().toLowerCase()))
  return (
    <div>
      <Input
        value={filter}
        onChange={(event) => setFilter(event.target.value)}
        placeholder={t('transitions.filter', 'Filter transitions')}
        aria-label={t('transitions.filter', 'Filter transitions')}
        className="mb-2 h-7 text-xs"
      />
      <div
        role="listbox"
        aria-label={t('transitions.picker', 'Transitions')}
        className="grid max-h-80 grid-cols-4 gap-1.5 overflow-y-auto pr-1 thin-scrollbar"
      >
        {defaultLabel !== null ? (
          <PickerTile isSelected={value === DEFAULT_TRANSITION_OPTION} label={defaultLabel} onSelect={() => onSelect(DEFAULT_TRANSITION_OPTION)}>
            <span className="text-[0.6rem] font-bold uppercase text-muted-foreground">{t('transitions.default', 'Default')}</span>
          </PickerTile>
        ) : null}
        <PickerTile isSelected={value === NO_TRANSITION} label={t('transitions.none', 'No transition')} onSelect={() => onSelect(NO_TRANSITION)}>
          <Ban className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
        </PickerTile>
        {types.map((type) => (
          <PickerTile key={type} isSelected={value === type} label={type} onSelect={() => onSelect(type)}>
            <span aria-hidden="true" style={transitionPreviewStyle(type)} className="h-full w-full" />
          </PickerTile>
        ))}
      </div>
    </div>
  )
}
