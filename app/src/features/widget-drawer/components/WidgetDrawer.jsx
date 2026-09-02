/**
 * Widgets catalog content for the shared left drawer.
 */

import { useLayoutStore, useActivityStore } from '@/hooks/useAppStoreSelectors'
import { useWidgetManager } from '@/features/widget-editor/hooks/useWidgetManager'
import { WidgetButtonGrid } from './WidgetButtonGrid'

/**
 * Provides Widgets content for the shared left drawer.
 * @returns {JSX.Element} Rendered React element.
 */
export function WidgetDrawerContent({ widgetLiveEdits }) {
  const { dismissLeftDrawerOverlay } = useLayoutStore()
  const { activitySummary } = useActivityStore()
  const { addWidget } = useWidgetManager({ widgetLiveEdits })

  const handleAddWidget = (request) => {
    addWidget(request)
    dismissLeftDrawerOverlay()
  }

  return <WidgetButtonGrid onAddWidget={handleAddWidget} availableMetrics={activitySummary?.availableMetrics ?? []} />
}
