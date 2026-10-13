import { Info } from 'lucide-react'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { cn } from '@/lib/utils'

/**
 * Renders a help icon with an immediate, wrapped tooltip.
 * @param {object} props - Help tooltip props.
 * @param {import('react').ReactNode} props.content - Localized help content.
 * @param {string} [props.ariaLabel=content] - Accessible button label; required for formatted content.
 * @param {string} [props.className=''] - Additional wrapper classes.
 * @param {string} [props.contentClassName=''] - Additional tooltip bubble classes.
 * @param {'top'|'bottom'|'left'|'right'} [props.side='bottom'] - Tooltip side.
 * @param {'start'|'center'|'end'} [props.align='start'] - Tooltip alignment.
 * @returns {JSX.Element} Help tooltip.
 */
export function HelpTooltip({ content, ariaLabel = content, className = '', contentClassName = '', side = 'bottom', align = 'start' }) {
  return (
    <SimpleTooltip
      content={content}
      side={side}
      align={align}
      delayMs={0}
      className={cn('z-20 shrink-0', className)}
      contentClassName={cn('w-[20rem] whitespace-pre-line', contentClassName)}
    >
      <button
        type="button"
        aria-label={ariaLabel}
        className="inline-flex cursor-help items-center justify-center rounded-full p-0.5 leading-none text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
      >
        <Info className="h-3.5 w-3.5 shrink-0 overflow-visible" aria-hidden="true" />
      </button>
    </SimpleTooltip>
  )
}
