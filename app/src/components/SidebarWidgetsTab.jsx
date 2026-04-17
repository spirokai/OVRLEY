import { useState, useMemo, useEffect } from 'react'
import {
  Gauge,
  Mountain,
  Map,
  TrendingUp,
  Tag,
  Trash2,
  Clock,
  Zap,
  Activity,
  RotateCcw,
  Move,
  Type,
  Palette,
  Timer,
  Thermometer,
} from 'lucide-react'
import { Label } from '@/components/ui/label'
import { Input } from '@/components/ui/input'
import { BlurInput } from '@/components/ui/blur-input'
import { Slider } from '@/components/ui/slider'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from '@/components/ui/accordion'
import useStore from '../store/useStore'
import { Button } from '@/components/ui/button'

const QUICKMENU_ITEMS = [
  { type: 'label', icon: Type, label: 'Text' },
  { type: 'speed', icon: Gauge, label: 'Speed' },
  { type: 'elevation', icon: Mountain, label: 'Elev.' },
  { type: 'heartrate', icon: Activity, label: 'HR' },
  { type: 'power', icon: Zap, label: 'Power' },
  { type: 'cadence', icon: Timer, label: 'Cadence' },
  { type: 'time', icon: Clock, label: 'Time' },
  { type: 'temperature', icon: Thermometer, label: 'Temp.' },
  { type: 'gradient', icon: TrendingUp, label: 'Grad.' },
  { type: 'course', icon: Map, label: 'Map' },
]

const TYPE_LABELS = {
  label: 'Text',
  speed: 'Speed',
  elevation: 'Elevation',
  heartrate: 'Heart Rate',
  power: 'Power',
  cadence: 'Cadence',
  time: 'Time',
  temperature: 'Temperature',
  gradient: 'Gradient',
  course: 'Route Map',
}

const TYPE_ICONS = {
  label: Type,
  speed: Gauge,
  elevation: Mountain,
  heartrate: Activity,
  power: Zap,
  cadence: Timer,
  time: Clock,
  temperature: Thermometer,
  gradient: TrendingUp,
  course: Map,
}

const FONTS = [
  { id: 'Arial.ttf', name: 'Arial' },
  { id: 'Evogria.otf', name: 'Evogria' },
  { id: 'Furore.otf', name: 'Furore' },
]

export default function SidebarWidgetsTab() {
  const { config, setConfig } = useStore()
  const [activeWidgetId, setActiveWidgetId] = useState(null)

  // Flatten and group widgets
  const widgets = useMemo(() => {
    if (!config) return []
    const all = []

    // Labels
    ;(config.labels || []).forEach((item, i) => {
      all.push({
        id: `label-${i}`,
        type: 'label',
        category: 'labels',
        index: i,
        name: item.text || 'Text',
        data: item,
      })
    })

    // Values
    ;(config.values || []).forEach((item, i) => {
      all.push({
        id: `value-${i}`,
        type: item.value,
        category: 'values',
        index: i,
        name: TYPE_LABELS[item.value] || item.value,
        data: item,
      })
    })

    // Plots
    ;(config.plots || []).forEach((item, i) => {
      all.push({
        id: `plot-${i}`,
        type: item.value,
        category: 'plots',
        index: i,
        name: `${TYPE_LABELS[item.value] || item.value} Chart`,
        data: item,
      })
    })

    // Group by type and sort alphabetically by type name
    const grouped = all.reduce((acc, widget) => {
      const typeName = TYPE_LABELS[widget.type] || widget.type
      if (!acc[typeName]) acc[typeName] = []
      acc[typeName].push(widget)
      return acc
    }, {})

    // Sort group names alphabetically
    const sortedTypes = Object.keys(grouped).sort()

    // Flatten back with group info
    const result = []
    sortedTypes.forEach((typeName, idx) => {
      grouped[typeName].forEach((widget) => {
        result.push({
          ...widget,
          isFirstInGroup: widget === grouped[typeName][0],
          isLastInGroup:
            widget === grouped[typeName][grouped[typeName].length - 1],
          showSeparator: idx > 0 && widget === grouped[typeName][0],
        })
      })
    })

    return result
  }, [config])

  // Set default active widget ONLY when the list changes or on mount
  useEffect(() => {
    if (widgets.length > 0) {
      if (!activeWidgetId) {
        setActiveWidgetId(widgets[0].id)
      } else {
        // Ensure the activeWidgetId still exists
        if (!widgets.find((w) => w.id === activeWidgetId)) {
          setActiveWidgetId(widgets[0].id)
        }
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [widgets.length]) // Removed activeWidgetId from deps to allow manual collapse

  const addWidget = (type) => {
    const newConfig = JSON.parse(JSON.stringify(config))
    let newId = ''

    if (type === 'label') {
      if (!newConfig.labels) newConfig.labels = []
      const newItem = {
        x: 100,
        y: 100,
        font_size: 60,
        text: 'New Text',
        font_family: 'Arial.ttf',
        color: '#ffffff',
        opacity: 1,
      }
      newConfig.labels.push(newItem)
      newId = `label-${newConfig.labels.length - 1}`
    } else if (
      [
        'speed',
        'elevation',
        'gradient',
        'course',
        'heartrate',
        'power',
        'cadence',
        'time',
        'temperature',
      ].includes(type)
    ) {
      if (['course', 'elevation', 'gradient'].includes(type)) {
        if (!newConfig.plots) newConfig.plots = []
        const newItem = {
          value: type,
          x: 100,
          y: 100,
          width: 400,
          height: 200,
          color: '#ffffff',
          opacity: 1,
        }
        newConfig.plots.push(newItem)
        newId = `plot-${newConfig.plots.length - 1}`
      } else {
        if (!newConfig.values) newConfig.values = []
        const newItem = {
          x: 100,
          y: 100,
          font_size: 100,
          value: type,
          unit: 'metric',
          font_family: 'Furore.otf',
          color: '#ffffff',
          opacity: 1,
          prefix: '',
          suffix: '',
          decimals: 0,
        }
        newConfig.values.push(newItem)
        newId = `value-${newConfig.values.length - 1}`
      }
    }

    setConfig(newConfig)
    if (newId) setActiveWidgetId(newId)
  }

  const deleteWidget = (id) => {
    const widget = widgets.find((w) => w.id === id)
    if (!widget) return

    const newConfig = JSON.parse(JSON.stringify(config))
    newConfig[widget.category] = newConfig[widget.category].filter(
      (_, i) => i !== widget.index,
    )

    setConfig(newConfig)
  }

  const updateWidgetData = (id, updates) => {
    const widget = widgets.find((w) => w.id === id)
    if (!widget) return

    const newConfig = JSON.parse(JSON.stringify(config))
    newConfig[widget.category][widget.index] = {
      ...newConfig[widget.category][widget.index],
      ...updates,
    }
    setConfig(newConfig)
  }

  const resetWidget = (id) => {
    const widget = widgets.find((w) => w.id === id)
    if (!widget) return

    let defaults = { x: 100, y: 100 }
    if (widget.type === 'label') {
      defaults = {
        ...defaults,
        font_size: 60,
        text: 'Text',
        font_family: 'Arial.ttf',
        color: '#ffffff',
        opacity: 1,
      }
    } else if (widget.category === 'plots') {
      defaults = {
        ...defaults,
        width: 400,
        height: 200,
        color: '#ffffff',
        opacity: 1,
      }
    } else {
      defaults = {
        ...defaults,
        font_size: 100,
        font_family: 'Furore.otf',
        color: '#ffffff',
        opacity: 1,
        prefix: '',
        suffix: '',
        decimals: 0,
      }
    }

    updateWidgetData(id, defaults)
  }

  if (!config) return null

  return (
    <div className="space-y-6">
      {/* Quickmenu */}
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

      {/* Widget Accordion */}
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
            collapsible
            value={activeWidgetId}
            onValueChange={setActiveWidgetId}
            className="space-y-1"
          >
            {widgets.map((widget) => {
              const Icon = TYPE_ICONS[widget.type] || Tag
              return (
                <div key={widget.id}>
                  {widget.showSeparator && (
                    <Separator className="my-3 bg-zinc-800/30" />
                  )}
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
                            {/* <span className="text-[9px] text-zinc-500 uppercase tracking-wider font-bold">
                              {widget.category.slice(0, -1)}
                            </span> */}
                          </div>
                        </div>
                      </AccordionTrigger>
                      <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center z-10">
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7 text-zinc-500 hover:text-red-500 hover:bg-red-500/10"
                          onClick={(e) => {
                            e.stopPropagation()
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
                          {/* Position Controls */}
                          <div className="space-y-3">
                            <div className="flex items-center gap-2">
                              <Move className="h-3.5 w-3.5 text-red-500" />
                              <h5 className="text-[10px] font-bold text-zinc-400 uppercase tracking-wider">
                                Position
                              </h5>
                            </div>
                            <div className="grid grid-cols-2 gap-3">
                              <div className="space-y-1">
                                <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                  X Position
                                </Label>
                                <BlurInput
                                  type="number"
                                  value={widget.data.x}
                                  onChange={(e) =>
                                    updateWidgetData(widget.id, {
                                      x: parseInt(e.target.value) || 0,
                                    })
                                  }
                                  className="h-8 text-xs bg-zinc-950/50 border-zinc-800"
                                />
                              </div>
                              <div className="space-y-1">
                                <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                  Y Position
                                </Label>
                                <BlurInput
                                  type="number"
                                  value={widget.data.y}
                                  onChange={(e) =>
                                    updateWidgetData(widget.id, {
                                      y: parseInt(e.target.value) || 0,
                                    })
                                  }
                                  className="h-8 text-xs bg-zinc-950/50 border-zinc-800"
                                />
                              </div>
                            </div>
                          </div>

                          {/* Dimensions for Plots */}
                          {widget.category === 'plots' && (
                            <div className="space-y-3">
                              <div className="flex items-center gap-2">
                                <TrendingUp className="h-3.5 w-3.5 text-red-500" />
                                <h5 className="text-[10px] font-bold text-zinc-400 uppercase tracking-wider">
                                  Dimensions
                                </h5>
                              </div>
                              <div className="grid grid-cols-2 gap-3">
                                <div className="space-y-1">
                                  <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                    Width
                                  </Label>
                                  <BlurInput
                                    type="number"
                                    value={widget.data.width}
                                    onChange={(e) =>
                                      updateWidgetData(widget.id, {
                                        width: parseInt(e.target.value) || 0,
                                      })
                                    }
                                    className="h-8 text-xs bg-zinc-950/50 border-zinc-800"
                                  />
                                </div>
                                <div className="space-y-1">
                                  <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                    Height
                                  </Label>
                                  <BlurInput
                                    type="number"
                                    value={widget.data.height}
                                    onChange={(e) =>
                                      updateWidgetData(widget.id, {
                                        height: parseInt(e.target.value) || 0,
                                      })
                                    }
                                    className="h-8 text-xs bg-zinc-950/50 border-zinc-800"
                                  />
                                </div>
                              </div>
                            </div>
                          )}

                          {/* Typography for Labels and Values */}
                          {(widget.category === 'labels' ||
                            widget.category === 'values') && (
                            <div className="space-y-4">
                              <div className="flex items-center gap-2">
                                <Type className="h-3.5 w-3.5 text-red-500" />
                                <h5 className="text-[10px] font-bold text-zinc-400 uppercase tracking-wider">
                                  Content & Font
                                </h5>
                              </div>

                              {widget.type === 'label' && (
                                <div className="space-y-1">
                                  <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                    Text Content
                                  </Label>
                                  <BlurInput
                                    value={widget.data.text}
                                    onChange={(e) =>
                                      updateWidgetData(widget.id, {
                                        text: e.target.value,
                                      })
                                    }
                                    className="h-8 text-xs bg-zinc-950/50 border-zinc-800"
                                  />
                                </div>
                              )}

                              <div className="grid grid-cols-2 gap-3">
                                <div className="space-y-1">
                                  <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                    Font Family
                                  </Label>
                                  <Select
                                    value={
                                      widget.data.font_family || 'Arial.ttf'
                                    }
                                    onValueChange={(v) =>
                                      updateWidgetData(widget.id, {
                                        font_family: v,
                                      })
                                    }
                                  >
                                    <SelectTrigger className="h-8 text-xs bg-zinc-950/50 border-zinc-800">
                                      <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                      {FONTS.map((f) => (
                                        <SelectItem key={f.id} value={f.id}>
                                          {f.name}
                                        </SelectItem>
                                      ))}
                                    </SelectContent>
                                  </Select>
                                </div>
                                <div className="space-y-1">
                                  <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                    Font Size
                                  </Label>
                                  <BlurInput
                                    type="number"
                                    value={widget.data.font_size}
                                    onChange={(e) =>
                                      updateWidgetData(widget.id, {
                                        font_size:
                                          parseInt(e.target.value) || 8,
                                      })
                                    }
                                    className="h-8 text-xs bg-zinc-950/50 border-zinc-800"
                                  />
                                </div>
                              </div>

                              <div className="space-y-2">
                                <Slider
                                  min={8}
                                  max={300}
                                  step={1}
                                  value={[widget.data.font_size]}
                                  onValueChange={([v]) =>
                                    updateWidgetData(widget.id, {
                                      font_size: v,
                                    })
                                  }
                                  className="py-2"
                                />
                              </div>

                              {widget.category === 'values' && (
                                <>
                                  <div className="grid grid-cols-2 gap-3">
                                    <div className="space-y-1">
                                      <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                        Prefix
                                      </Label>
                                      <BlurInput
                                        value={widget.data.prefix || ''}
                                        onChange={(e) =>
                                          updateWidgetData(widget.id, {
                                            prefix: e.target.value,
                                          })
                                        }
                                        className="h-8 text-xs bg-zinc-950/50 border-zinc-800"
                                        placeholder="e.g. Speed:"
                                      />
                                    </div>
                                    <div className="space-y-1">
                                      <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                        Suffix
                                      </Label>
                                      <BlurInput
                                        value={widget.data.suffix || ''}
                                        onChange={(e) =>
                                          updateWidgetData(widget.id, {
                                            suffix: e.target.value,
                                          })
                                        }
                                        className="h-8 text-xs bg-zinc-950/50 border-zinc-800"
                                        placeholder="e.g. km/h"
                                      />
                                    </div>
                                  </div>
                                  <div className="grid grid-cols-2 gap-3">
                                    <div className="space-y-1">
                                      <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                        Decimals
                                      </Label>
                                      <BlurInput
                                        type="number"
                                        min={0}
                                        max={3}
                                        value={widget.data.decimals ?? 0}
                                        onChange={(e) =>
                                          updateWidgetData(widget.id, {
                                            decimals:
                                              parseInt(e.target.value) || 0,
                                          })
                                        }
                                        className="h-8 text-xs bg-zinc-950/50 border-zinc-800"
                                      />
                                    </div>
                                    <div className="space-y-1">
                                      <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                        Unit System
                                      </Label>
                                      <Select
                                        value={widget.data.unit || 'metric'}
                                        onValueChange={(v) =>
                                          updateWidgetData(widget.id, {
                                            unit: v,
                                          })
                                        }
                                      >
                                        <SelectTrigger className="h-8 text-xs bg-zinc-950/50 border-zinc-800">
                                          <SelectValue />
                                        </SelectTrigger>
                                        <SelectContent>
                                          <SelectItem value="metric">
                                            Metric
                                          </SelectItem>
                                          <SelectItem value="imperial">
                                            Imperial
                                          </SelectItem>
                                        </SelectContent>
                                      </Select>
                                    </div>
                                  </div>
                                </>
                              )}
                            </div>
                          )}

                          {/* Appearance / Colors */}
                          <div className="space-y-3">
                            <div className="flex items-center gap-2">
                              <Palette className="h-3.5 w-3.5 text-red-500" />
                              <h5 className="text-[10px] font-bold text-zinc-400 uppercase tracking-wider">
                                Appearance
                              </h5>
                            </div>

                            <div className="grid grid-cols-2 gap-3">
                              <div className="space-y-2">
                                <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                  Color
                                </Label>
                                <div className="flex gap-2">
                                  <Input
                                    type="color"
                                    value={widget.data.color || '#ffffff'}
                                    onChange={(e) =>
                                      updateWidgetData(widget.id, {
                                        color: e.target.value,
                                      })
                                    }
                                    className="w-10 h-8 p-1 bg-zinc-950 border-zinc-800 cursor-pointer"
                                  />
                                  <BlurInput
                                    value={widget.data.color || '#ffffff'}
                                    onChange={(e) =>
                                      updateWidgetData(widget.id, {
                                        color: e.target.value,
                                      })
                                    }
                                    className="h-8 text-xs font-mono bg-zinc-950/50 border-zinc-800 flex-1"
                                  />
                                </div>
                              </div>
                              <div className="space-y-2">
                                <div className="flex justify-between items-center">
                                  <Label className="text-[9px] text-zinc-500 uppercase font-bold">
                                    Opacity
                                  </Label>
                                  <span className="text-[10px] font-mono text-zinc-400">
                                    {Math.round(
                                      (widget.data.opacity || 1) * 100,
                                    )}
                                    %
                                  </span>
                                </div>
                                <Slider
                                  min={0}
                                  max={1}
                                  step={0.05}
                                  value={[widget.data.opacity || 1]}
                                  onValueChange={([v]) =>
                                    updateWidgetData(widget.id, {
                                      opacity: v,
                                    })
                                  }
                                  className="py-2"
                                />
                              </div>
                            </div>
                          </div>
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
