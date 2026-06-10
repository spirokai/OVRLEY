/**
 * WidgetButtonGrid — scrollable categorized grid of widget-type buttons inside the drawer.
 */

import { GROUPED_QUICKMENU_ITEMS } from '@/lib/widget-icons'

function WidgetButton({ item, onClick }) {
  const Icon = item.icon
  return (
    <button
      onClick={() => onClick(item.type)}
      className="group flex flex-col items-center justify-center gap-2 w-full aspect-square rounded-lg border border-border/70 bg-surface transition-all hover:border-accent-border hover:bg-surface-accent-soft cursor-pointer"
    >
      <Icon className="h-5 w-5 text-muted-foreground group-hover:text-primary" />
      <span className="text-[9px] leading-tight text-foreground text-center px-0.5 group-hover:text-primary">{item.label}</span>
    </button>
  )
}

/**
 * Renders a scrollable 2-column grid of widget-type buttons, grouped by category.
 *
 * @param {object} props
 * @param {(type: string) => void} props.onAddWidget — Called with the widget type when a button is clicked.
 * @returns {JSX.Element} Rendered React element.
 */
export function WidgetButtonGrid({ onAddWidget }) {
  return (
    <div className="flex-1 overflow-y-auto thin-scrollbar p-2">
      {GROUPED_QUICKMENU_ITEMS.map((group) => (
        <div key={group.category} className="mb-3">
          <div className="text-[9px] font-bold uppercase tracking-wider text-muted-foreground/60 px-1 mb-1.5">{group.category}</div>
          <div className="grid grid-cols-3 gap-2">
            {group.items.map((item) => (
              <WidgetButton key={item.type} item={item} onClick={onAddWidget} />
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}
