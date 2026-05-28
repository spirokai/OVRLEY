/**
 * WidgetDrawer — collapsible left-side panel for widgets.
 */

import { useLayoutStore } from '@/hooks/useAppStoreSelectors'
import { useWidgetManager } from '@/features/widget-editor/hooks/useWidgetManager'
import { WidgetButtonGrid } from './WidgetButtonGrid'
import { useEffect } from 'react'

/**
 * Provides widget drawer.
 * @returns {JSX.Element} Rendered React element.
 */
export function WidgetDrawer() {
  const { widgetDrawerOpen, toggleWidgetDrawer } = useLayoutStore()
  const { addWidget } = useWidgetManager()

  useEffect(() => {
    if (!widgetDrawerOpen) return

    function handleKeyDown(e) {
      if (e.key === 'Escape') {
        toggleWidgetDrawer()
      }
    }

    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [widgetDrawerOpen, toggleWidgetDrawer])

  return (
    <div className="absolute top-6 bottom-6 left-0 pointer-events-none z-60 ">
      <div
        className="h-full flex transition-transform duration-300 ease-in-out pointer-events-auto "
        style={{
          transform: widgetDrawerOpen ? 'translateX(0)' : 'translateX(calc(-100% + 24px))',
        }}
      >
        <div className="w-40 h-full bg-card rounded-r-lg flex flex-col overflow-hidden border border-border/60 ">
          <WidgetButtonGrid onAddWidget={addWidget} />
        </div>
        <div className="flex flex-col h-full">
          <div style={{ height: '15%' }} />
          <button
            onClick={toggleWidgetDrawer}
            className="flex items-center justify-center w-6 h-25 bg-primary text-primary-foreground rounded-r-md cursor-pointer shrink-0"
            aria-label={widgetDrawerOpen ? 'Close widget drawer' : 'Open widget drawer'}
          >
            <span className="[writing-mode:vertical-lr] rotate-180 text-[10px] font-bold tracking-wider">WIDGETS</span>
          </button>
          <div className="flex-1" />
        </div>
      </div>
    </div>
  )
}
