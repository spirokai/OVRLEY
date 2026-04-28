import { Move, Palette, TrendingUp, Type } from 'lucide-react'
import {
  ColorField,
  NumberField,
  SelectField,
  SliderField,
  TextField,
  TIME_FORMATS,
  ToggleField,
} from './widgetFormControls'
import FontSelectField from '@/components/ui/font-select-field'
import useAvailableFonts from '@/hooks/useAvailableFonts'
import { createFontSelection } from '@/lib/fonts'
import { getWidgetFont } from './widgetDefinitions'
import { getThemeColor } from '@/lib/theme'

export function SectionHeading({ icon: Icon, title }) {
  return (
    <div className="flex items-center gap-2">
      <Icon className="h-3.5 w-3.5 text-primary" />
      <h5 className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
        {title}
      </h5>
    </div>
  )
}

export function PositionSection({ widget, setNumericField }) {
  return (
    <div className="space-y-3">
      <SectionHeading icon={Move} title="Position" />
      <div className="grid grid-cols-2 gap-3">
        <NumberField
          label="X Position"
          value={widget.data.x ?? 0}
          onChange={(rawValue) => setNumericField(widget.id, 'x', rawValue)}
        />
        <NumberField
          label="Y Position"
          value={widget.data.y ?? 0}
          onChange={(rawValue) => setNumericField(widget.id, 'y', rawValue)}
        />
      </div>
    </div>
  )
}

export function DimensionsSection({ widget, setNumericField }) {
  return (
    <div className="space-y-3">
      <SectionHeading icon={TrendingUp} title="Dimensions" />
      <div className="grid grid-cols-2 gap-3">
        <NumberField
          label="Width"
          value={widget.data.width ?? 0}
          onChange={(rawValue) =>
            setNumericField(widget.id, 'width', rawValue, { min: 0 })
          }
        />
        <NumberField
          label="Height"
          value={widget.data.height ?? 0}
          onChange={(rawValue) =>
            setNumericField(widget.id, 'height', rawValue, { min: 0 })
          }
        />
      </div>
    </div>
  )
}

export function FontSection({
  widget,
  updateWidgetData,
  title = 'Typography',
  showTextInput = false,
  fontSizeLabel = 'Font Size',
  sizeMin = 8,
  sizeMax = 300,
  colorLabel = 'Font Color',
  showFormatSelect = false,
}) {
  const fontSize = widget.data.font_size ?? 60
  const systemFonts = useAvailableFonts()

  return (
    <div className="space-y-4">
      <SectionHeading icon={Type} title={title} />

      {showTextInput ? (
        <TextField
          label="Text"
          value={widget.data.text || ''}
          onChange={(value) => updateWidgetData(widget.id, { text: value })}
        />
      ) : null}

      {showFormatSelect ? (
        <SelectField
          label="Format"
          value={widget.data.format || 'time-24'}
          onValueChange={(value) =>
            updateWidgetData(widget.id, { format: value })
          }
          options={TIME_FORMATS}
        />
      ) : null}

      <div className="grid grid-cols-2 gap-3">
        <FontSelectField
          label="Font Family"
          value={getWidgetFont(widget)}
          onValueChange={(value) =>
            updateWidgetData(widget.id, createFontSelection(value))
          }
          systemFonts={systemFonts}
          triggerClassName="h-9 border-border/70 bg-surface text-xs"
          labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
        />
        <ColorField
          label={colorLabel}
          value={widget.data.color || getThemeColor('ice')}
          onChange={(value) => updateWidgetData(widget.id, { color: value })}
        />
      </div>

      <SliderField
        label={fontSizeLabel}
        value={fontSize}
        min={sizeMin}
        max={sizeMax}
        step={1}
        valueDisplay={`${fontSize}px`}
        onSliderChange={(value) =>
          updateWidgetData(widget.id, { font_size: value })
        }
      />
    </div>
  )
}

export function IconSection({
  widget,
  updateWidgetData,
  setNumericField,
  title = 'Icon & Units',
  showUnitsToggle = false,
  unitsField = null,
}) {
  const iconSize = widget.data.icon_size ?? 28

  return (
    <div className="space-y-4">
      <SectionHeading icon={Palette} title={title} />
      <ToggleField
        label="Display Icon"
        checked={widget.data.show_icon ?? true}
        onCheckedChange={(checked) =>
          updateWidgetData(widget.id, { show_icon: checked })
        }
      />
      <div className="grid grid-cols-2 gap-3">
        <ColorField
          label="Icon Color"
          value={widget.data.icon_color || getThemeColor('aqua')}
          onChange={(value) =>
            updateWidgetData(widget.id, { icon_color: value })
          }
        />
        <SliderField
          label="Icon Size"
          value={iconSize}
          min={0}
          max={100}
          step={1}
          valueDisplay={`${iconSize}px`}
          onSliderChange={(value) =>
            updateWidgetData(widget.id, { icon_size: value })
          }
        />
      </div>
      <div className="grid grid-cols-2 gap-3">
        <NumberField
          label="Icon Offset X"
          value={widget.data.icon_offset_x ?? 0}
          onChange={(rawValue) =>
            setNumericField(widget.id, 'icon_offset_x', rawValue)
          }
        />
        <NumberField
          label="Icon Offset Y"
          value={widget.data.icon_offset_y ?? 0}
          onChange={(rawValue) =>
            setNumericField(widget.id, 'icon_offset_y', rawValue)
          }
        />
      </div>

      {showUnitsToggle && unitsField ? (
        unitsField
      ) : showUnitsToggle ? (
        <ToggleField
          label="Display Units"
          checked={widget.data.show_units ?? true}
          onCheckedChange={(checked) =>
            updateWidgetData(widget.id, { show_units: checked })
          }
        />
      ) : null}
    </div>
  )
}

export function OpacitySection({ widget, updateWidgetData }) {
  const opacity = Math.round((widget.data.opacity ?? 1) * 100)

  return (
    <div className="space-y-3">
      <SectionHeading icon={Palette} title="Opacity" />
      <SliderField
        label="Widget Opacity"
        value={opacity}
        min={0}
        max={100}
        step={1}
        valueDisplay={`${opacity}%`}
        onSliderChange={(value) =>
          updateWidgetData(widget.id, { opacity: value / 100 })
        }
      />
    </div>
  )
}
