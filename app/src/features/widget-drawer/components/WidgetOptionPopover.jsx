import { useState } from 'react'
import { Popover, PopoverTrigger, PopoverContent } from '@/components/ui/popover'

/**
 * Presents the canonical creation options for a widget catalog entry.
 *
 * @param {object} props
 * @param {Array<{value: string, label: string, icon: React.ComponentType, selection: object}>} props.options - Available creation options.
 * @param {(selection: object) => void} props.onSelect - Called with the selected creation fields.
 * @param {React.ReactNode} props.children - Popover trigger.
 * @returns {JSX.Element} Rendered React element.
 */
export function WidgetOptionPopover({ options, onSelect, children }) {
  const [open, setOpen] = useState(false)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent side="right" align="start" sideOffset={6.8} className="z-70 -ml-2 mt-6 w-48 rounded-none p-[0.2rem]">
        <div className="flex flex-col gap-[0.11rem]">
          {options.map((option) => {
            const Icon = option.icon
            return (
              <button
                key={option.value}
                onClick={() => {
                  onSelect(option.selection)
                  setOpen(false)
                }}
                className="group flex items-center gap-4 px-[0.4rem] py-[0.3rem] rounded-none hover:bg-accent text-[0.85rem] hover:text-accent-foreground cursor-pointer text-left"
              >
                <Icon className="h-4.5 w-4.5 shrink-0 text-muted-foreground group-hover:text-accent-foreground" />
                <span>{option.label}</span>
              </button>
            )
          })}
        </div>
      </PopoverContent>
    </Popover>
  )
}
