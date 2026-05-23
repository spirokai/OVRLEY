/**
 * WidgetDrawer — collapsible left-side panel for widgets.
 */

import { Grid3X3 } from 'lucide-react'
import { useEffect } from 'react'
import { useLayoutStore } from '@/hooks/useAppStoreSelectors'

/**
 * Provides widget drawer.
 * @returns {JSX.Element} Rendered React element.
 */
export function WidgetDrawer() {
  const { widgetDrawerOpen, toggleWidgetDrawer } = useLayoutStore()

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
    <div className="absolute top-2 bottom-2 left-0 pointer-events-none z-500 ">
      <div
        className="h-full flex transition-transform duration-300 ease-in-out pointer-events-auto "
        style={{
          transform: widgetDrawerOpen ? 'translateX(0)' : 'translateX(calc(-100% + 24px))',
        }}
      >
        <div className="w-40 h-full bg-card rounded-r-md z-500" />
        <div className="flex flex-col h-full">
          <div style={{ height: '15%' }} />
          <button
            onClick={toggleWidgetDrawer}
            className="flex items-center justify-center w-6 h-25 bg-card rounded-r-sm cursor-pointer shrink-0"
            aria-label={widgetDrawerOpen ? 'Close widget drawer' : 'Open widget drawer'}
          >
            <Grid3X3 className="h-3 w-3" />
          </button>
          <div className="flex-1" />
        </div>
      </div>
    </div>
  )
}
