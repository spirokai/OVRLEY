import { useState } from 'react'
import { Ban, ChevronDown } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import { NO_TRANSITION, XFADE_TRANSITIONS } from '@/lib/video-program/transition-catalog'
import { DEFAULT_TRANSITION_OPTION } from '../utils/joinTransitionModel'
import { transitionPreviewUrl } from '../utils/transitionPreviews'

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
 * Popover picker of a transition: an animated preview per `xfade` transition, no
 * transition, and (for joins) the project default.
 *
 * @param {object} props Picker props.
 * @param {string} props.value Selected option: an `xfade` name, `none`, or `default`.
 * @param {string} props.label Trigger label of the effective transition.
 * @param {string|null} props.previewType Transition shown on the trigger, or null.
 * @param {string|null} [props.defaultLabel] Label of the default option; omitted for the project default itself.
 * @param {boolean} [props.disabled] Disables the trigger.
 * @param {string} [props.id] Trigger element ID.
 * @param {(value: string) => void} props.onSelect Called with the chosen option.
 */
export function TransitionSelect({ value, label, previewType, defaultLabel = null, disabled = false, id, onSelect }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [filter, setFilter] = useState('')
  const choose = (option) => {
    setOpen(false)
    setFilter('')
    onSelect(option)
  }
  const types = XFADE_TRANSITIONS.filter((type) => type.includes(filter.trim().toLowerCase()))
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          id={id}
          type="button"
          variant="outline"
          disabled={disabled}
          className="h-7 w-full min-w-0 justify-between gap-2 px-2 text-xs font-normal"
        >
          <span className="flex min-w-0 items-center gap-2">
            {previewType !== null ? <img src={transitionPreviewUrl(previewType)} alt="" className="h-4 w-7 shrink-0 rounded-[2px]" /> : null}
            <span className="truncate">{label}</span>
          </span>
          <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-60" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="z-200 w-104 p-3">
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
            <PickerTile isSelected={value === DEFAULT_TRANSITION_OPTION} label={defaultLabel} onSelect={() => choose(DEFAULT_TRANSITION_OPTION)}>
              <span className="text-[0.6rem] font-bold uppercase text-muted-foreground">{t('transitions.default', 'Default')}</span>
            </PickerTile>
          ) : null}
          <PickerTile isSelected={value === NO_TRANSITION} label={t('transitions.none', 'No transition')} onSelect={() => choose(NO_TRANSITION)}>
            <Ban className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
          </PickerTile>
          {types.map((type) => (
            <PickerTile key={type} isSelected={value === type} label={type} onSelect={() => choose(type)}>
              <img src={transitionPreviewUrl(type)} alt="" loading="lazy" className="h-full w-full object-cover" />
            </PickerTile>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  )
}
