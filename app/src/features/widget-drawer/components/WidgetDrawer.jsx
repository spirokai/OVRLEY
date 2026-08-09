/**
 * WidgetDrawer — collapsible left-side panel for widgets.
 */

import { useEffect, useEffectEvent } from 'react'
import { matchKeyboardShortcut } from '@/lib/keyboard-shortcuts'
import { useLayoutStore, useActivityStore } from '@/hooks/useAppStoreSelectors'
import { useWidgetManager } from '@/features/widget-editor/hooks/useWidgetManager'
import { WidgetButtonGrid } from './WidgetButtonGrid'

/**
 * Provides widget drawer.
 * @returns {JSX.Element} Rendered React element.
 */
export function WidgetDrawer({ widgetLiveEdits }) {
  const { closeWidgetDrawer, widgetDrawerOpen, toggleWidgetDrawer } = useLayoutStore()
  const { activitySummary } = useActivityStore()
  const { addWidget } = useWidgetManager({ widgetLiveEdits })

  const handleAddWidget = (request) => {
    addWidget(request)
    closeWidgetDrawer()
  }

  const onKeyDown = useEffectEvent((event) => {
    if (event.defaultPrevented) return
    const match = matchKeyboardShortcut(event, 'drawer')
    if (match?.commandId !== 'drawer.close') return

    event.preventDefault()
    closeWidgetDrawer()
  })

  useEffect(() => {
    if (!widgetDrawerOpen || typeof document === 'undefined') return undefined

    const handleKeyDown = (event) => onKeyDown(event)
    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [widgetDrawerOpen])

  return (
    <>
      {widgetDrawerOpen ? <div className="fixed inset-0 z-50" data-testid="widget-drawer-backdrop" onClick={closeWidgetDrawer} /> : null}
      <div className="absolute top-6 bottom-6 left-0 pointer-events-none z-60 ">
        <div
          className="h-full flex transition-transform duration-300 ease-in-out pointer-events-auto "
          style={{
            transform: widgetDrawerOpen ? 'translateX(0)' : 'translateX(calc(-100% + 24px))',
          }}
        >
          <div className="w-60 h-full bg-card rounded-r-sm flex flex-col overflow-hidden border border-border/60 shadow-lg shadow-black/80">
            <WidgetButtonGrid
              onAddWidget={handleAddWidget}
              validAttributes={activitySummary?.validAttributes}
              extendedAttributes={activitySummary?.extendedAttributes}
            />
          </div>
          <div className="flex flex-col h-full">
            <div style={{ height: '15%' }} />
            <button
              onClick={toggleWidgetDrawer}
              className="flex items-center justify-center w-6 h-25 bg-primary text-primary-foreground rounded-r-xs cursor-pointer shrink-0"
              aria-label={widgetDrawerOpen ? 'Close widget drawer' : 'Open widget drawer'}
              aria-keyshortcuts="Alt+W"
            >
              <span className="[writing-mode:vertical-lr] rotate-180 text-xs font-bold tracking-wider">WIDGETS</span>
            </button>
            <div className="flex-1" />
          </div>
        </div>
      </div>
    </>
  )
}
