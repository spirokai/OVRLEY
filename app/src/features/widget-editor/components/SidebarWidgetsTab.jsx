/**
 * Renders the sidebar widgets tab portion of the application interface.
 * Presentation only — all state and CRUD logic is owned by useWidgetManager.
 */

import { RotateCcw, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { TYPE_ICONS } from '@/lib/widget/widget-icons'
import { isStandardMetricWidgetType } from '@/lib/widget/standard-metrics'
import { useWidgetManager } from '../hooks/useWidgetManager'
import { PositionSection } from './widgetEditorSections'
import ElevationWidgetEditor from './ElevationWidgetEditor'
import GradientWidgetEditor from './GradientWidgetEditor'
import MetricWidgetEditor from './metricWidget/MetricWidgetEditor'
import RouteMapWidgetEditor from './RouteMapWidgetEditor'
import TextWidgetEditor from './TextWidgetEditor'
import TimeWidgetEditor from './TimeWidgetEditor'

/**
 * Widget type → editor component dispatch map.
 *
 * Each entry maps a widget type to its dedicated editor component.
 * Standard metric widgets (speed, cadence, heartrate, power, etc.) are
 * handled by MetricWidgetEditor and checked first via
 * isStandardMetricWidgetType().
 */
const WIDGET_EDITOR_MAP = {
  label: TextWidgetEditor,
  time: TimeWidgetEditor,
  gradient: GradientWidgetEditor,
  course: RouteMapWidgetEditor,
  elevation: ElevationWidgetEditor,
}

/**
 * Resolves the appropriate editor component for a widget type.
 *
 * @param {object} widget - Widget definition being rendered or edited.
 * @param {Function} updateWidgetData - Callback to update widget data.
 * @param {Function} setNumericField - Callback to set a numeric field on a widget.
 * @param {number} [sceneFontSize] - Scene fallback font size.
 * @returns {JSX.Element|null} Rendered editor component or null.
 */
function renderWidgetEditor(widget, updateWidgetData, setNumericField, sceneFontSize) {
  // Specialized editors take priority over the generic metric editor
  const Editor = WIDGET_EDITOR_MAP[widget.type]
  if (Editor) {
    return <Editor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} sceneFontSize={sceneFontSize} />
  }
  if (isStandardMetricWidgetType(widget.type)) {
    return <MetricWidgetEditor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} />
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
    <div className="space-y-6">
      <div className="space-y-3">
        {widgets.length === 0 ? (
          <div className="rounded-lg border border-dashed border-border/70 py-8 text-center">
            <p className="text-xs text-muted-foreground">No active widgets</p>
          </div>
        ) : (
          <Accordion
            type="single"
            collapsible
            className="border-b border-border/70"
            value={selectedWidgetId || undefined}
            onValueChange={(value) => setSelectedWidgetId(value || null)}
          >
            {widgets.map((widget) => {
              const Icon = TYPE_ICONS[widget.type] || TYPE_ICONS.label

              return (
                <div key={widget.id}>
                  <AccordionItem
                    value={widget.id}
                    className="overflow-hidden border border-transparent border-b-none transition-all data-[state=open]:border-accent-border hover:border-primary"
                  >
                    <div className="relative group">
                      <AccordionTrigger className="group w-full px-3 py-3 pr-10 border-t border-border/70 data-[state=open]:border-r-transparent hover:no-underline data-[state=open]:text-primary data-[state=open]:bg-surface-accent-soft hover:text-primary ">
                        <div className="flex items-center gap-2.5 flex-1 min-w-0">
                          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded group-hover:bg-surface-accent-strong ">
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

                    <AccordionContent className="px-4 pb-6 pt-1.5 bg-surface/60 ">
                      <div className="space-y-6">
                        <PositionSection
                          widget={widget}
                          setNumericField={setNumericField}
                          updateWidgetData={updateWidgetData}
                          headerAction={
                            <Button
                              variant="ghost"
                              size="icon"
                              className="h-6 w-6 text-muted-foreground hover:text-foreground hover:bg-foreground/20"
                              onClick={() => resetWidget(widget.id)}
                            >
                              <RotateCcw className="h-3 w-3" />
                            </Button>
                          }
                        />
                        {renderWidgetEditor(widget, updateWidgetData, setNumericField, config?.scene?.font_size)}
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
