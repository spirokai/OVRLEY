/**
 * Renders the sidebar widgets tab portion of the application interface.
 * Presentation only — all state and CRUD logic is owned by useWidgetManager.
 */

import { RotateCcw, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { TYPE_ICONS } from '@/lib/widget-icons'
import { isStandardMetricWidgetType } from '@/lib/standard-metrics'
import { useWidgetManager } from '../hooks/useWidgetManager'
import { PositionSection } from './widgetEditorSections'
import ElevationWidgetEditor from './ElevationWidgetEditor'
import GradientWidgetEditor from './GradientWidgetEditor'
import HeadingWidgetEditor from './HeadingWidgetEditor'
import MetricWidgetEditor from './MetricWidgetEditor'
import RouteMapWidgetEditor from './RouteMapWidgetEditor'
import TextWidgetEditor from './TextWidgetEditor'
import TimeWidgetEditor from './TimeWidgetEditor'

/**
 * Maps widget type to its editor component.
 *
 * @param {object} widget - Widget definition being rendered or edited.
 * @param {Function} updateWidgetData - Callback to update widget data.
 * @param {Function} setNumericField - Callback to set a numeric field on a widget.
 * @param {number} [sceneFontSize] - Scene fallback font size.
 * @returns {JSX.Element|null} Rendered editor component or null.
 */
function renderWidgetEditor(widget, updateWidgetData, setNumericField, sceneFontSize) {
  if (widget.type === 'label') {
    return <TextWidgetEditor widget={widget} updateWidgetData={updateWidgetData} />
  }

  if (isStandardMetricWidgetType(widget.type)) {
    return <MetricWidgetEditor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} />
  }

  if (widget.type === 'time') {
    return <TimeWidgetEditor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} />
  }

  if (widget.type === 'gradient') {
    return <GradientWidgetEditor widget={widget} updateWidgetData={updateWidgetData} />
  }

  if (widget.type === 'course') {
    return <RouteMapWidgetEditor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} />
  }

  if (widget.type === 'elevation') {
    return (
      <ElevationWidgetEditor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} sceneFontSize={sceneFontSize} />
    )
  }

  if (widget.type === 'heading') {
    return <HeadingWidgetEditor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} />
  }

  return null
}

/**
 * Renders the sidebar widgets tab component.
 * @returns {JSX.Element} Rendered component output.
 */
export default function SidebarWidgetsTab() {
  const { config, widgets, selectedWidgetId, updateWidgetData, setNumericField, deleteWidget, resetWidget, setSelectedWidgetId } = useWidgetManager()

  if (!config) return null

  return (
    <div className="space-y-6 pt-4">
      <div className="space-y-3">
        <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">Active Widgets</h4>

        {widgets.length === 0 ? (
          <div className="rounded-lg border border-dashed border-border/70 py-8 text-center">
            <p className="text-xs text-muted-foreground">No widgets added yet.</p>
          </div>
        ) : (
          <Accordion
            type="single"
            value={selectedWidgetId || undefined}
            onValueChange={(value) => setSelectedWidgetId(value || widgets[0]?.id || null)}
            className="space-y-1"
          >
            {widgets.map((widget) => {
              const Icon = TYPE_ICONS[widget.type] || TYPE_ICONS.label

              return (
                <div key={widget.id} className="space-y-1">
                  <AccordionItem
                    value={widget.id}
                    className="overflow-hidden rounded-lg border border-border/60 bg-surface/80 transition-all data-[state=open]:border-accent-border  hover:border-primary "
                  >
                    <div className="relative group">
                      <AccordionTrigger className="group w-full px-3 py-2 pr-10 hover:no-underline data-[state=open]:text-primary data-[state=open]:bg-surface-accent-soft hover:text-primary ">
                        <div className="flex items-center gap-2.5 flex-1 min-w-0">
                          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded bg-surface-strong group-hover:bg-surface-accent-strong group-data-[state=open]:bg-surface-accent-strong">
                            <Icon className="h-3.5 w-3.5 text-muted-foreground group-hover:text-primary group-data-[state=open]:text-primary" />
                          </div>
                          <div className="flex flex-col items-start gap-0.5 min-w-0 flex-1">
                            <span className="w-full truncate text-left text-xs font-semibold group-data-[state=open]:text-primary">
                              {widget.name}
                            </span>
                          </div>
                        </div>
                      </AccordionTrigger>
                      <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center z-10">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7 text-muted-foreground group-data-[state=open]/item:text-primary hover:bg-surface-accent-soft hover:text-primary"
                          onClick={(event) => {
                            event.stopPropagation()
                            deleteWidget(widget.id)
                          }}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </div>

                    <AccordionContent className="px-4 pb-3 pt-1.5 ">
                      <div className="relative">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="absolute top-1 -right-1 h-6 w-6 text-muted-foreground hover:text-foreground"
                          onClick={() => resetWidget(widget.id)}
                        >
                          <RotateCcw className="h-3 w-3" />
                        </Button>

                        <div className="space-y-6 pt-4">
                          <PositionSection widget={widget} setNumericField={setNumericField} updateWidgetData={updateWidgetData} />
                          {renderWidgetEditor(widget, updateWidgetData, setNumericField, config?.scene?.font_size)}
                        </div>
                      </div>
                    </AccordionContent>
                  </AccordionItem>
                </div>
              )
            })}
          </Accordion>
        )}
      </div>
    </div>
  )
}
