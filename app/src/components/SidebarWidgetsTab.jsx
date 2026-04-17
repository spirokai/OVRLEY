import { useEffect, useMemo, useState } from 'react'
import { RotateCcw, Tag, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from '@/components/ui/accordion'
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
  const { config, setConfig, globalDefaults } = useStore()
  const [activeWidgetId, setActiveWidgetId] = useState(null)

  const widgets = useMemo(() => {
    if (!config) return []

    const all = []

    ;(config.labels || []).forEach((item, index) => {
      all.push({
        id: `label-${index}`,
        type: 'label',
        category: 'labels',
        index,
        name: item.text || 'Text',
        data: item,
      })
    })
    ;(config.values || []).forEach((item, index) => {
      all.push({
        id: `value-${index}`,
        type: item.value,
        category: 'values',
        index,
        name: TYPE_LABELS[item.value] || item.value,
        data: item,
      })
    })
    ;(config.plots || []).forEach((item, index) => {
      all.push({
        id: `plot-${index}`,
        type: item.value,
        category: 'plots',
        index,
        name: TYPE_LABELS[item.value] || item.value,
        data: item,
      })
    })

    const grouped = all.reduce((accumulator, widget) => {
      const typeName = TYPE_LABELS[widget.type] || widget.type
      if (!accumulator[typeName]) accumulator[typeName] = []
      accumulator[typeName].push(widget)
      return accumulator
    }, {})

    return Object.keys(grouped)
      .sort()
      .flatMap((typeName, groupIndex) =>
        grouped[typeName].map((widget, widgetIndex) => ({
          ...widget,
          showSeparator: groupIndex > 0 && widgetIndex === 0,
        })),
      )
  }, [config])

  useEffect(() => {
    if (widgets.length === 0) {
      if (activeWidgetId !== null) setActiveWidgetId(null)
      return
    }

    if (!activeWidgetId) {
      setActiveWidgetId(widgets[widgets.length - 1].id)
      return
    }

    if (!widgets.some((widget) => widget.id === activeWidgetId)) {
      setActiveWidgetId(widgets[0].id)
    }
  }, [widgets, activeWidgetId])

  const updateWidgetData = (id, updates) => {
    const widget = widgets.find((item) => item.id === id)
    if (!widget) return

    const nextConfig = JSON.parse(JSON.stringify(config))
    nextConfig[widget.category][widget.index] = {
      ...nextConfig[widget.category][widget.index],
      ...updates,
    }
    setConfig(nextConfig)
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
      nextConfig.plots.push(createPlotDefaults(type, globalDefaults))
      newId = `plot-${nextConfig.plots.length - 1}`
    }

    setConfig(nextConfig)
    if (newId) setActiveWidgetId(newId)
  }

  const deleteWidget = (id) => {
    const widget = widgets.find((item) => item.id === id)
    if (!widget) return

    const nextConfig = JSON.parse(JSON.stringify(config))
    nextConfig[widget.category] = nextConfig[widget.category].filter(
      (_, index) => index !== widget.index,
    )

    setConfig(nextConfig)
  }

  const resetWidget = (id) => {
    const widget = widgets.find((item) => item.id === id)
    if (!widget) return

    if (widget.type === 'label') {
      updateWidgetData(id, createLabelDefaults(globalDefaults))
      return
    }

    if (widget.type === 'course' || widget.type === 'elevation') {
      updateWidgetData(id, createPlotDefaults(widget.type, globalDefaults))
      return
    }

    updateWidgetData(id, createMetricValueDefaults(widget.type, globalDefaults))
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
              className="h-12 w-full bg-zinc-900/50 border-zinc-800 hover:bg-red-500/10 hover:border-red-500/50 transition-all group"
              onClick={() => addWidget(item.type)}
            >
              <item.icon className="h-5 w-5 text-zinc-400 group-hover:text-red-500" />
            </Button>
          ))}
        </div>
      </div>

      <Separator className="bg-zinc-800/50" />

      <div className="space-y-3">
        <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">
          Active Widgets
        </h4>

        {widgets.length === 0 ? (
          <div className="text-center py-8 border border-dashed border-zinc-800 rounded-lg">
            <p className="text-xs text-muted-foreground">
              No widgets added yet.
            </p>
          </div>
        ) : (
          <Accordion
            type="single"
            value={activeWidgetId || undefined}
            onValueChange={(value) =>
              setActiveWidgetId(value || widgets[0]?.id || null)
            }
            className="space-y-1"
          >
            {widgets.map((widget) => {
              const Icon = TYPE_ICONS[widget.type] || Tag

              return (
                <div key={widget.id}>
                  {widget.showSeparator ? (
                    <Separator className="my-3 bg-zinc-800/30" />
                  ) : null}
                  <AccordionItem
                    value={widget.id}
                    className="border border-zinc-800/50 rounded-lg bg-zinc-900/30 overflow-hidden data-[state=open]:border-red-500/30 data-[state=open]:bg-red-500/5 transition-all"
                  >
                    <div className="relative group">
                      <AccordionTrigger className="w-full px-3 py-2.5 pr-10 hover:no-underline transition-colors group">
                        <div className="flex items-center gap-2.5 flex-1 min-w-0">
                          <div className="shrink-0 w-7 h-7 rounded bg-zinc-800/50 flex items-center justify-center group-hover:bg-red-500/20 transition-colors">
                            <Icon className="h-3.5 w-3.5 text-zinc-400 group-hover:text-red-500" />
                          </div>
                          <div className="flex flex-col items-start gap-0.5 min-w-0 flex-1">
                            <span className="text-xs font-semibold text-zinc-200 truncate w-full text-left">
                              {widget.name}
                            </span>
                          </div>
                        </div>
                      </AccordionTrigger>
                      <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center z-10">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7 text-zinc-500 hover:text-red-500 hover:bg-red-500/10"
                          onClick={(event) => {
                            event.stopPropagation()
                            deleteWidget(widget.id)
                          }}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </div>

                    <AccordionContent className="px-4 pb-4 pt-2 border-t border-zinc-800/50">
                      <div className="relative">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="absolute -top-1 -right-1 h-6 w-6 text-zinc-600 hover:text-zinc-200"
                          onClick={() => resetWidget(widget.id)}
                        >
                          <RotateCcw className="h-3 w-3" />
                        </Button>

                        <div className="space-y-4 pt-2">
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
