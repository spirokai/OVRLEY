import { useState } from 'react'
import { ChevronDown } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { transitionPreviewStyle } from '../utils/transitionPreviews'
import { TransitionPicker } from './TransitionPicker'

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
  const [open, setOpen] = useState(false)
  const choose = (option) => {
    setOpen(false)
    onSelect(option)
  }
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          id={id}
          type="button"
          variant="outline"
          disabled={disabled}
          className="h-7 w-full min-w-0 justify-between gap-2 border-border/70 bg-transparent px-2 text-xs font-normal hover:border-accent-border hover:bg-surface-highlight-soft"
        >
          <span className="flex min-w-0 items-center gap-2">
            {previewType !== null ? (
              <span aria-hidden="true" style={transitionPreviewStyle(previewType)} className="h-4 w-7 shrink-0 rounded-[2px]" />
            ) : null}
            <span className="truncate">{label}</span>
          </span>
          <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-60" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="z-200 w-104 p-3">
        <TransitionPicker value={value} defaultLabel={defaultLabel} onSelect={choose} />
      </PopoverContent>
    </Popover>
  )
}
