/**
 * WidgetButtonGrid — scrollable categorized grid of widget-type buttons inside the drawer.
 */

import { GROUPED_QUICKMENU_ITEMS } from '@/lib/widget/widget-icons'
import { WidgetOptionPopover } from './WidgetOptionPopover'

function WidgetButton({ item, onClick, isAvailable }) {
  const Icon = item.icon
  const hasMultipleOptions = item.options.length > 1

  const button = (
    <button
      onClick={hasMultipleOptions ? undefined : () => onClick({ type: item.type, ...item.options[0].selection })}
      className="group relative flex flex-col items-center justify-center gap-2 w-full aspect-square rounded-xs border border-border/70 bg-surface transition-all hover:border-accent-border hover:bg-surface-accent-soft/30 cursor-pointer overflow-hidden"
    >
      {isAvailable && (
        <span
          aria-hidden="true"
          className="absolute top-0 right-0 h-[6px] w-[6px] bg-success"
          style={{ clipPath: 'polygon(100% 0, 0 0, 100% 100%)' }}
        />
      )}
      <Icon className="h-5 w-5 text-muted-foreground group-hover:text-primary" />
      <span className="text-[9px] leading-tight text-foreground text-center px-0.5 group-hover:text-primary">{item.label}</span>
    </button>
  )

  if (!hasMultipleOptions) return button

  return (
    <WidgetOptionPopover options={item.options} onSelect={(selection) => onClick({ type: item.type, ...selection })}>
      {button}
    </WidgetOptionPopover>
  )
}

/**
 * Renders a scrollable 3-column grid of widget-type buttons, grouped by category.
 *
 * @param {object} props
 * @param {(request: {type: string, displayType?: string, lapTimerMode?: string}) => void} props.onAddWidget - Called with a canonical creation request.
 * @param {string[]} [props.validAttributes] — Core metric attribute names available in the loaded activity.
 * @param {string[]} [props.extendedAttributes] — Extended metric attribute names available in the loaded activity.
 * @returns {JSX.Element} Rendered React element.
 */
export function WidgetButtonGrid({ onAddWidget, validAttributes = [], extendedAttributes = [] }) {
  const availableSet = new Set(['label', 'backdrop', ...(validAttributes || []), ...(extendedAttributes || [])])

  return (
    <div className="flex-1 overflow-y-auto thin-scrollbar p-2">
      {GROUPED_QUICKMENU_ITEMS.map((group) => (
        <div key={group.category} className="mb-3">
          <div className="text-[9px] font-bold uppercase tracking-wider text-muted-foreground/60 px-1 mb-1.5">{group.category}</div>
          <div className="grid grid-cols-3 gap-2">
            {group.items.map((item) => (
              <WidgetButton key={item.type} item={item} onClick={onAddWidget} isAvailable={availableSet.has(item.type)} />
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}
