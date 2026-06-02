/**
 * WidgetButtonGrid — scrollable grid of widget-type buttons inside the drawer.
 */

import { QUICKMENU_ITEMS } from '@/lib/widget-icons'

/**
 * Renders a scrollable 2-column grid of widget-type buttons.
 *
 * @param {object} props
 * @param {(type: string) => void} props.onAddWidget — Called with the widget type when a button is clicked.
 * @returns {JSX.Element} Rendered React element.
 */
export function WidgetButtonGrid({ onAddWidget }) {
  return (
    <div className="flex-1 overflow-y-auto thin-scrollbar p-2">
      <div className="grid grid-cols-2 gap-2">
        {QUICKMENU_ITEMS.map((item) => {
          const Icon = item.icon

          return (
            <button
              key={item.type}
              onClick={() => onAddWidget(item.type)}
              className="group flex flex-col items-center justify-center gap-2 w-full aspect-square rounded-lg border border-border/70 bg-surface transition-all hover:border-accent-border hover:bg-surface-accent-soft cursor-pointer"
            >
              <Icon className="h-5 w-5 text-muted-foreground group-hover:text-primary" />
              <span className="text-[8px] leading-tight font-light text-muted-foreground text-center px-0.5 group-hover:text-primary">
                {item.label}
              </span>
            </button>
          )
        })}
      </div>
    </div>
  )
}
