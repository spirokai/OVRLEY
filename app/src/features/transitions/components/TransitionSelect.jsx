import { useState } from 'react'
import { ChevronDownIcon } from 'lucide-react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { SELECT_TRIGGER_CLASS } from '@/components/ui/select'
import { cn } from '@/lib/utils'
import { transitionPreviewStyle } from '../utils/transitionPreviews'
import { TransitionPicker } from './TransitionPicker'

/**
 * Popover picker of a transition: an animated preview per `xfade` transition, no
 * transition, and (for joins) the project default. Its trigger looks like every
 * other select of the app.
 *
 * @param {object} props Picker props.
 * @param {string} props.value Selected option: an `xfade` name, `none`, or `default`.
 * @param {string} props.label Trigger label of the effective transition.
 * @param {string|null} props.previewType Transition shown on the trigger, or null.
 * @param {string|null} [props.defaultLabel] Label of the default option; omitted for the project default itself.
 * @param {boolean} [props.disabled] Disables the trigger.
 * @param {string} [props.id] Trigger element ID.
 * @param {'default'|'sm'} [props.size] Trigger height, as for a select.
 * @param {(value: string) => void} props.onSelect Called with the chosen option.
 */
export function TransitionSelect({ value, label, previewType, defaultLabel = null, disabled = false, id, size = 'default', onSelect }) {
  const [open, setOpen] = useState(false)
  const choose = (option) => {
    setOpen(false)
    onSelect(option)
  }
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button id={id} type="button" disabled={disabled} data-size={size} className={cn(SELECT_TRIGGER_CLASS, 'min-w-0 text-xs')}>
          <span className="flex min-w-0 items-center gap-2">
            {previewType !== null ? (
              <span aria-hidden="true" style={transitionPreviewStyle(previewType)} className="h-4 w-7 shrink-0 rounded-[2px]" />
            ) : null}
            <span className="truncate">{label}</span>
          </span>
          <ChevronDownIcon className="size-4 opacity-50" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="z-200 w-104 p-3">
        <TransitionPicker value={value} defaultLabel={defaultLabel} onSelect={choose} />
      </PopoverContent>
    </Popover>
  )
}
