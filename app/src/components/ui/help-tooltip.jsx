import { Info } from 'lucide-react'
import { SimpleTooltip } from '@/components/ui/simple-tooltip'
import { cn } from '@/lib/utils'

/**
 * Renders a help icon with an immediate, wrapped tooltip.
 * @param {object} props - Help tooltip props.
 * @param {string} props.content - Localized help text and accessible button label.
 * @param {string} [props.className=''] - Additional wrapper classes.
 * @param {string} [props.contentClassName=''] - Additional tooltip bubble classes.
 * @param {'top'|'bottom'|'left'|'right'} [props.side='bottom'] - Tooltip side.
 * @param {'start'|'center'|'end'} [props.align='start'] - Tooltip alignment.
 * @returns {JSX.Element} Help tooltip.
 */
export function HelpTooltip({ content, className = '', contentClassName = '', side = 'bottom', align = 'start' }) {
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
        aria-label={content}
        className="inline-flex cursor-help items-center justify-center rounded-full p-0.5 leading-none text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
      >
        <Info className="h-3.5 w-3.5 shrink-0 overflow-visible" aria-hidden="true" />
      </button>
    </SimpleTooltip>
  )
}
