import { useEffect, useMemo } from 'react'
import { RotateCcw, Tag, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from '@/components/ui/accordion'
import { getCurrentParsedActivity } from '../api/activityCache'
import useStore from '../store/useStore'
import ElevationWidgetEditor from './widgets/ElevationWidgetEditor'
import GradientWidgetEditor from './widgets/GradientWidgetEditor'
import MetricWidgetEditor from './widgets/MetricWidgetEditor'
import RouteMapWidgetEditor from './widgets/RouteMapWidgetEditor'
import TemperatureWidgetEditor from './widgets/TemperatureWidgetEditor'
import TextWidgetEditor from './widgets/TextWidgetEditor'
import TimeWidgetEditor from './widgets/TimeWidgetEditor'
import {
  createLabelDefaults,
  createMetricValueDefaults,
  createPlotDefaults,
  clamp,
  parseInteger,
  QUICKMENU_ITEMS,
  TYPE_ICONS,
  TYPE_LABELS,
} from './widgets/widgetDefinitions'
import {
  buildConfigWidgets,
  deleteWidgetInConfig,
  groupWidgetsForSidebar,
  replaceWidgetInConfig,
  updateWidgetInConfig,
} from '@/lib/widget-config'
import { PositionSection } from './widgets/widgetEditorSections'

function renderWidgetEditor(widget, updateWidgetData, setNumericField) {
  if (widget.type === 'label') {
    return (
      <TextWidgetEditor widget={widget} updateWidgetData={updateWidgetData} />
    )
  }

  if (['speed', 'heartrate', 'cadence', 'power'].includes(widget.type)) {
    return (
      <MetricWidgetEditor
        widget={widget}
        updateWidgetData={updateWidgetData}
        setNumericField={setNumericField}
      />
    )
  }

  if (widget.type === 'time') {
    return (
      <TimeWidgetEditor
        widget={widget}
        updateWidgetData={updateWidgetData}
        setNumericField={setNumericField}
      />
    )
  }

  if (widget.type === 'temperature') {
    return (
      <TemperatureWidgetEditor
        widget={widget}
        updateWidgetData={updateWidgetData}
        setNumericField={setNumericField}
      />
    )
  }

  if (widget.type === 'gradient') {
    return (
      <GradientWidgetEditor
        widget={widget}
        updateWidgetData={updateWidgetData}
      />
    )
  }

  if (widget.type === 'course') {
    return (
      <RouteMapWidgetEditor
        widget={widget}
        updateWidgetData={updateWidgetData}
        setNumericField={setNumericField}
      />
    )
  }

  if (widget.type === 'elevation') {
    return (
      <ElevationWidgetEditor
        widget={widget}
        updateWidgetData={updateWidgetData}
        setNumericField={setNumericField}
      />
    )
  }

  return null
}

export default function SidebarWidgetsTab() {
  const {
    config,
    setConfig,
    globalDefaults,
    selectedWidgetId,
    setSelectedWidgetId,
  } = useStore()
  const parsedActivity = getCurrentParsedActivity()

  const widgets = useMemo(() => {
    return groupWidgetsForSidebar(buildConfigWidgets(config), TYPE_LABELS)
  }, [config])

  useEffect(() => {
    if (widgets.length === 0) {
      if (selectedWidgetId !== null) setSelectedWidgetId(null)
      return
    }

    if (!selectedWidgetId) {
      setSelectedWidgetId(widgets[widgets.length - 1].id)
      return
    }

    if (!widgets.some((widget) => widget.id === selectedWidgetId)) {
      setSelectedWidgetId(widgets[0].id)
    }
  }, [widgets, selectedWidgetId, setSelectedWidgetId])

  const updateWidgetData = (id, updates) => {
    setConfig(updateWidgetInConfig(config, id, updates))
  }

  const setNumericField = (widgetId, key, rawValue, options = {}) => {
    const { fallback = 0, min, max } = options
    const parsed = parseInteger(rawValue, fallback)
    const nextValue =
      min !== undefined || max !== undefined
        ? clamp(parsed, min ?? parsed, max ?? parsed)
        : parsed

    updateWidgetData(widgetId, { [key]: nextValue })
  }

  const addWidget = (type) => {
    const nextConfig = JSON.parse(JSON.stringify(config))
    let newId = ''

    if (type === 'label') {
      if (!nextConfig.labels) nextConfig.labels = []
      nextConfig.labels.push(createLabelDefaults(globalDefaults))
      newId = `label-${nextConfig.labels.length - 1}`
    } else if (
      [
        'speed',
        'gradient',
        'heartrate',
        'power',
        'cadence',
        'time',
        'temperature',
      ].includes(type)
    ) {
      if (!nextConfig.values) nextConfig.values = []
      nextConfig.values.push(createMetricValueDefaults(type, globalDefaults))
      newId = `value-${nextConfig.values.length - 1}`
    } else if (['course', 'elevation'].includes(type)) {
      if (!nextConfig.plots) nextConfig.plots = []
      nextConfig.plots.push(
        createPlotDefaults(type, globalDefaults, {
          coursePoints: parsedActivity?.sample_course_points,
        }),
      )
      newId = `plot-${nextConfig.plots.length - 1}`
    }

    setConfig(nextConfig)
    if (newId) setSelectedWidgetId(newId)
  }

  const deleteWidget = (id) => {
    setConfig(deleteWidgetInConfig(config, id))
  }

  const resetWidget = (id) => {
    const widget = widgets.find((item) => item.id === id)
    if (!widget) return

    if (widget.type === 'label') {
      setConfig(
        replaceWidgetInConfig(config, id, createLabelDefaults(globalDefaults)),
      )
      return
    }

    if (widget.type === 'course' || widget.type === 'elevation') {
      setConfig(
        replaceWidgetInConfig(
          config,
          id,
          createPlotDefaults(widget.type, globalDefaults),
        ),
      )
      return
    }

    setConfig(
      replaceWidgetInConfig(
        config,
        id,
        createMetricValueDefaults(widget.type, globalDefaults),
      ),
    )
  }

  if (!config) return null

  return (
    <div className="space-y-6">
      <div className="space-y-3">
        <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">
          Quick Add
        </h4>
        <div className="grid grid-cols-5 gap-3">
          {QUICKMENU_ITEMS.map((item) => (
            <Button
              key={item.type}
              variant="outline"
              size="icon"
              className="h-12 w-full border-border/70 bg-surface transition-all group hover:border-accent-border hover:bg-surface-accent-soft"
              onClick={() => addWidget(item.type)}
            >
              <item.icon className="h-5 w-5 text-muted-foreground group-hover:text-primary" />
            </Button>
          ))}
        </div>
      </div>

      <Separator className="bg-border/60" />

      <div className="space-y-3">
        <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">
          Active Widgets
        </h4>

        {widgets.length === 0 ? (
          <div className="rounded-lg border border-dashed border-border/70 py-8 text-center">
            <p className="text-xs text-muted-foreground">
              No widgets added yet.
            </p>
          </div>
        ) : (
          <Accordion
            type="single"
            value={selectedWidgetId || undefined}
            onValueChange={(value) =>
              setSelectedWidgetId(value || widgets[0]?.id || null)
            }
            className="space-y-1"
          >
            {widgets.map((widget) => {
              const Icon = TYPE_ICONS[widget.type] || Tag

              return (
                <div key={widget.id} className="space-y-1">
                  <AccordionItem
                    value={widget.id}
                    className="overflow-hidden rounded-lg border border-border/60 bg-surface/80 transition-all data-[state=open]:border-accent-border data-[state=open]:bg-surface-accent-soft"
                  >
                    <div className="relative group">
                      <AccordionTrigger className="group w-full px-3 py-2 pr-10 transition-colors hover:no-underline data-[state=open]:text-primary">
                        <div className="flex items-center gap-2.5 flex-1 min-w-0">
                          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded bg-surface-strong transition-colors group-hover:bg-surface-accent-strong group-data-[state=open]:bg-surface-accent-strong">
                            <Icon className="h-3.5 w-3.5 text-muted-foreground group-hover:text-primary group-data-[state=open]:text-primary" />
                          </div>
                          <div className="flex flex-col items-start gap-0.5 min-w-0 flex-1">
                            <span className="w-full truncate text-left text-xs font-semibold text-foreground group-data-[state=open]:text-primary">
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

                    <AccordionContent className="px-4 pb-3 pt-1.5">
                      <div className="relative">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="absolute top-1 -right-1 h-6 w-6 text-muted-foreground hover:text-foreground"
                          onClick={() => resetWidget(widget.id)}
                        >
                          <RotateCcw className="h-3 w-3" />
                        </Button>

                        <div className="space-y-3 pt-1">
                          <PositionSection
                            widget={widget}
                            setNumericField={setNumericField}
                          />
                          {renderWidgetEditor(
                            widget,
                            updateWidgetData,
                            setNumericField,
                          )}
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
