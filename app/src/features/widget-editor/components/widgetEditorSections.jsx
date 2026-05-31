/**
 * Shared section components used across widget editors.
 * Each section is a reusable UI block (Position, Dimensions, Font, Icon, Units).
 */

import { Move, Palette, Ruler, TrendingUp, Type } from 'lucide-react'
import { ColorField, NumberField, SelectField, SliderField, TextField, TIME_FORMATS, ToggleField } from './widgetFormControls'
import FontSelectField from '@/components/ui/font-select-field'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { createFontSelection } from '@/lib/fonts'
import { getWidgetFont } from '../utils/widgetUtils'
import { getThemeColor } from '@/lib/theme'

/**
 * Renders the section heading component.
 *
 * @param {object} props - Component props.
 * @param {*} props.icon - Value for icon.
 * @param {*} props.title - Value for title.
 * @returns {JSX.Element} Rendered component output.
 */
export function SectionHeading({ icon: Icon, title }) {
  return (
    <div className="flex items-center gap-2">
      <Icon className="h-3.5 w-3.5 text-primary" />
      <h5 className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">{title}</h5>
    </div>
  )
}

/**
 * Renders the position section component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @returns {JSX.Element} Rendered component output.
 */
export function PositionSection({ widget, setNumericField, updateWidgetData }) {
  const opacity = Math.round((widget.data.opacity ?? 1) * 100)

  return (
    <div className="space-y-3">
      <SectionHeading icon={Move} title="General" />
      <div className="grid grid-cols-2 gap-3">
        <NumberField label="Horizontal Position" value={widget.data.x ?? 0} onChange={(rawValue) => setNumericField(widget.id, 'x', rawValue)} />
        <NumberField label="Vertical Position" value={widget.data.y ?? 0} onChange={(rawValue) => setNumericField(widget.id, 'y', rawValue)} />
      </div>
      <SliderField
        label="Transparency"
        value={opacity}
        min={0}
        max={100}
        step={1}
        valueDisplay={`${opacity}%`}
        onSliderChange={(value) => updateWidgetData(widget.id, { opacity: value / 100 })}
      />
    </div>
  )
}

/**
 * Renders the dimensions section component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export function DimensionsSection({ widget, setNumericField }) {
  return (
    <div className="space-y-3">
      <SectionHeading icon={TrendingUp} title="Dimensions" />
      <div className="grid grid-cols-2 gap-3">
        <NumberField
          label="Width"
          value={widget.data.width ?? 0}
          onChange={(rawValue) => setNumericField(widget.id, 'width', rawValue, { min: 0 })}
        />
        <NumberField
          label="Height"
          value={widget.data.height ?? 0}
          onChange={(rawValue) => setNumericField(widget.id, 'height', rawValue, { min: 0 })}
        />
      </div>
    </div>
  )
}

/**
 * Renders the font section component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.title - Value for title.
 * @param {*} props.showTextInput - Boolean flag for show text input.
 * @param {*} props.fontSizeLabel - Value for font size label.
 * @param {*} props.sizeMin - Value for size min.
 * @param {*} props.sizeMax - Value for size max.
 * @param {*} props.colorLabel - Value for color label.
 * @param {*} props.showFormatSelect - Boolean flag for show format select.
 * @returns {JSX.Element} Rendered component output.
 */
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
  const availableFonts = useAvailableFonts()

  return (
    <div className="space-y-4">
      <SectionHeading icon={Type} title={title} />

      {showTextInput ? (
        <TextField label="Text" value={widget.data.text || ''} onChange={(value) => updateWidgetData(widget.id, { text: value })} />
      ) : null}

      {showFormatSelect ? (
        <SelectField
          label="Format"
          value={widget.data.format || 'time-24'}
          onValueChange={(value) => updateWidgetData(widget.id, { format: value })}
          options={TIME_FORMATS}
        />
      ) : null}

      <SliderField
        label={fontSizeLabel}
        value={fontSize}
        min={sizeMin}
        max={sizeMax}
        step={1}
        valueDisplay={`${fontSize}px`}
        onSliderChange={(value) => updateWidgetData(widget.id, { font_size: value })}
      />
      <div className="grid grid-cols-2 gap-3 items-end">
        <FontSelectField
          label="Font Family"
          value={getWidgetFont(widget)}
          onValueChange={(value) => updateWidgetData(widget.id, createFontSelection(value))}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName="h-9 border-border/70 bg-surface text-xs"
          labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
        />
        <ColorField
          label={colorLabel}
          value={widget.data.color || getThemeColor('ice')}
          onChange={(value) => updateWidgetData(widget.id, { color: value })}
        />
      </div>
    </div>
  )
}

/**
 * Renders the icon section component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @param {*} props.title - Value for title.
 * @param {*} props.showUnitsToggle - Boolean flag for show units toggle.
 * @param {*} props.unitsField - Value for units field.
 * @returns {JSX.Element} Rendered component output.
 */
export function IconSection({ widget, updateWidgetData, setNumericField, title = 'Icon', showUnitsToggle = false, unitsField = null }) {
  const iconSize = widget.data.icon_size ?? 28

  return (
    <div className="space-y-4">
      <div className="flex w-full justify-between items-center">
        <SectionHeading icon={Palette} title={title} />
        <ToggleField checked={widget.data.show_icon ?? true} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_icon: checked })} />
      </div>
      <div className="grid grid-cols-2 gap-3">
        <ColorField
          label="Color"
          disabled={!widget.data.show_icon}
          value={widget.data.icon_color || getThemeColor('aqua')}
          onChange={(value) => updateWidgetData(widget.id, { icon_color: value })}
        />
        <SliderField
          label="Size"
          value={iconSize}
          disabled={!widget.data.show_icon}
          min={0}
          max={100}
          step={1}
          valueDisplay={`${iconSize}px`}
          onSliderChange={(value) => updateWidgetData(widget.id, { icon_size: value })}
        />
      </div>
      <div className="grid grid-cols-2 gap-3">
        <NumberField
          disabled={!widget.data.show_icon}
          label="Horizontal Offset"
          value={widget.data.icon_offset_x ?? 0}
          onChange={(rawValue) => setNumericField(widget.id, 'icon_offset_x', rawValue)}
        />
        <NumberField
          disabled={!widget.data.show_icon}
          label="Vertical Offset"
          value={widget.data.icon_offset_y ?? 0}
          onChange={(rawValue) => setNumericField(widget.id, 'icon_offset_y', rawValue)}
        />
      </div>

      {showUnitsToggle && unitsField ? (
        unitsField
      ) : showUnitsToggle ? (
        <ToggleField
          label="Display Units"
          checked={widget.data.show_units ?? true}
          onCheckedChange={(checked) => updateWidgetData(widget.id, { show_units: checked })}
        />
      ) : null}
    </div>
  )
}

/**
 * Renders a control row with optional toggle, color picker, and unit selector.
 *
 * Settled contract: always pass explicit `checked` and `onCheckedChange` values.
 * The old convenience path (widget + updateWidgetData) has been removed.
 *
 * @param {object} props - Component props.
 * @param {boolean} props.checked - Whether the toggle is on.
 * @param {Function} props.onCheckedChange - Callback for toggle changes.
 * @param {string} [props.title='Unit'] - Section heading title.
 * @param {boolean} [props.showToggle=true] - Whether to show the toggle.
 * @param {string} [props.value] - Current select field value.
 * @param {Function} [props.onValueChange] - Callback for select changes.
 * @param {Array} [props.options] - Select field options.
 * @param {string} [props.selectLabel='Unit'] - Select field label.
 * @param {string} [props.colorValue] - Current color picker value.
 * @param {Function} [props.onColorChange] - Callback for color changes.
 * @param {string} [props.colorLabel='Color'] - Color field label.
 * @returns {JSX.Element} Rendered component output.
 */
export function UnitsControlRow({
  checked,
  onCheckedChange,
  title = 'Unit',
  showToggle = true,
  value,
  onValueChange,
  options,
  selectLabel = 'Unit',
  colorValue,
  onColorChange,
  colorLabel = 'Color',
}) {
  const showSelect = Array.isArray(options) && options.length > 0 && value !== undefined && typeof onValueChange === 'function'
  const showColor = colorValue !== undefined && typeof onColorChange === 'function'
  const controlsDisabled = showToggle && !checked

  return (
    <div className="space-y-2 pt-4">
      <div className="flex w-full justify-between items-center">
        <SectionHeading icon={Ruler} title={title} />
        {showToggle ? <ToggleField checked={checked} onCheckedChange={onCheckedChange} /> : null}
      </div>
      {showSelect || showColor ? (
        <div className="grid grid-cols-2 gap-3 items-start">
          {showColor ? <ColorField label={colorLabel} value={colorValue} onChange={onColorChange} disabled={controlsDisabled} /> : null}
          {showSelect ? (
            <SelectField label={selectLabel} value={value} onValueChange={onValueChange} options={options} disabled={controlsDisabled} />
          ) : null}
        </div>
      ) : null}
    </div>
  )
}
