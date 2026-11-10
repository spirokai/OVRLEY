import { Info } from 'lucide-react'
import { Tooltip as TooltipPrimitive } from 'radix-ui'
import { cn } from '@/lib/utils'

/**
 * Renders a help icon with an immediate, wrapped tooltip. The tooltip is portaled
 * above the whole app, so a scrolling panel does not clip it, and it opens on
 * hover and on keyboard focus.
 * @param {object} props - Help tooltip props.
 * @param {import('react').ReactNode} props.content - Localized help content.
 * @param {string} [props.ariaLabel=content] - Accessible button label; required for formatted content.
 * @param {string} [props.className=''] - Additional button classes.
 * @param {string} [props.contentClassName=''] - Additional tooltip bubble classes.
 * @param {'top'|'bottom'|'left'|'right'} [props.side='bottom'] - Preferred tooltip side; it flips when there is no room.
 * @param {'start'|'center'|'end'} [props.align='start'] - Tooltip alignment.
 * @returns {JSX.Element} Help tooltip.
 */
export function HelpTooltip({ content, ariaLabel = content, className = '', contentClassName = '', side = 'bottom', align = 'start' }) {
  return (
    <TooltipPrimitive.Provider delayDuration={0}>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>
          <button
            type="button"
            aria-label={ariaLabel}
            className={cn(
              'inline-flex shrink-0 cursor-help items-center justify-center rounded-full p-0.5 leading-none text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring',
              className,
            )}
          >
            <Info className="h-3.5 w-3.5 shrink-0 overflow-visible" aria-hidden="true" />
          </button>
        </TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal>
          <TooltipPrimitive.Content
            side={side}
            align={align}
            sideOffset={6}
            collisionPadding={8}
            className={cn(
              'z-1000 w-[20rem] whitespace-pre-line rounded border border-border/70 bg-surface-tooltip px-2.5 py-1.5 text-xs text-foreground shadow-2xl animate-in fade-in duration-100',
              contentClassName,
            )}
          >
            {content}
            <TooltipPrimitive.Arrow className="fill-surface-tooltip" />
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  )
}
