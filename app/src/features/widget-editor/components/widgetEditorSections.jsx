/**
 * Shared section components used across widget editors.
 * Each section is a reusable UI block (Position, Dimensions, Font, Icon, Units).
 */

import { Move, Palette, Ruler, TrendingUp, Type } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { ColorField, NumberField, SelectField, SizeSlider, SliderField, TextField, TIME_FORMATS, ToggleField } from './widgetFormControls'
import FontSelectField from '@/components/ui/font-select-field'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import { createFontSelection } from '@/lib/fonts'
import { getWidgetFont } from '../utils/widgetUtils'
import { getThemeColor } from '@/lib/theme'
import { useTranslation } from 'react-i18next'
import { translateOptions } from '@/i18n'

/**
 * Renders the position section component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @returns {JSX.Element} Rendered component output.
 */
export function PositionSection({ widget, setNumericField, updateWidgetData, headerAction = null }) {
  const { t } = useTranslation()
  const opacity = Math.round(widget.data.opacity * 100)
  const usesHorizontalAnchor =
    widget.category === 'values' && widget.data.display_type === 'text' && widget.type !== 'gradient' && widget.type !== 'lap_timer'

  return (
    <div className="space-y-4">
      <SectionHeading icon={Move} title={t('widget-editor.general', 'General')} trailing={headerAction} />
      <div className="grid grid-cols-2 gap-4 pt-2">
        <NumberField
          label={
            usesHorizontalAnchor
              ? t('widget-editor.horizontalAnchor', 'Horizontal Anchor')
              : t('widget-editor.horizontalPosition', 'Horizontal Position')
          }
          value={widget.data.x}
          onChange={(rawValue) => setNumericField(widget.id, 'x', rawValue)}
        />
        <NumberField
          label={t('widget-editor.verticalPosition', 'Vertical Position')}
          value={widget.data.y}
          onChange={(rawValue) => setNumericField(widget.id, 'y', rawValue)}
        />
      </div>
      <SliderField
        label={t('widget-editor.transparency', 'Transparency')}
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
  const { t } = useTranslation()
  return (
    <div className="space-y-3">
      <SectionHeading icon={TrendingUp} title={t('widget-editor.dimensions', 'Dimensions')} />
      <div className="grid grid-cols-2 gap-4">
        <NumberField
          label={t('widget-editor.width', 'Width')}
          value={widget.data.width}
          onChange={(rawValue) => setNumericField(widget.id, 'width', rawValue, { min: 0 })}
        />
        <NumberField
          label={t('widget-editor.height', 'Height')}
          value={widget.data.height}
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
 * @param {*} props.updateWidgetSize - Live size updater.
 * @param {*} props.commitWidgetSize - Commits a live size update.
 * @param {string} [props.title] - Section heading title. Defaults to the translated "Typography".
 * @param {boolean} [props.showTextInput=false] - Whether to show the text input.
 * @param {string} [props.fontSizeLabel] - Label for the font size slider. Defaults to the translated "Font Size".
 * @param {number} [props.sizeMin=20] - Minimum font size.
 * @param {number} [props.sizeMax=200] - Maximum font size.
 * @param {string} [props.colorLabel] - Label for the font color picker. Defaults to the translated "Font Color".
 * @param {boolean} [props.showFormatSelect=false] - Whether to show the format select.
 * @returns {JSX.Element} Rendered component output.
 */
export function FontSection({
  widget,
  updateWidgetData,
  updateWidgetSize,
  commitWidgetSize,
  title,
  showTextInput = false,
  fontSizeLabel,
  sizeMin = 20,
  sizeMax = 200,
  colorLabel,
  showFormatSelect = false,
}) {
  const { t } = useTranslation()
  const fontSize = widget.data.font_size
  const availableFonts = useAvailableFonts()

  return (
    <div className="space-y-4">
      <SectionHeading icon={Type} title={title} titleKey="widget-editor.typography" defaultTitle="Typography" />

      {showTextInput ? (
        <TextField
          label={t('widget-editor.text', 'Text')}
          value={widget.data.text || ''}
          onChange={(value) => updateWidgetData(widget.id, { text: value })}
        />
      ) : null}

      {showFormatSelect ? (
        <SelectField
          label={t('widget-editor.format', 'Format')}
          value={widget.data.format}
          onValueChange={(value) => updateWidgetData(widget.id, { format: value })}
          options={translateOptions(TIME_FORMATS, t)}
        />
      ) : null}

      <SizeSlider
        label={fontSizeLabel ?? t('widget-editor.fontSize', 'Font Size')}
        value={fontSize}
        min={sizeMin}
        max={sizeMax}
        step={1}
        valueDisplay={`${fontSize}px`}
        onChange={(value) => updateWidgetSize(widget.id, { font_size: value })}
        onCommit={() => commitWidgetSize(widget.id)}
      />
      <div className="grid grid-cols-2 gap-4 items-end">
        <FontSelectField
          label={t('widget-editor.fontFamily', 'Font Family')}
          value={getWidgetFont(widget)}
          onValueChange={(value) => updateWidgetData(widget.id, createFontSelection(value))}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName="h-9 border-border/70 bg-surface text-xs"
          labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
        />
        <ColorField
          label={colorLabel}
          labelKey="widget-editor.fontColor"
          defaultLabel="Font Color"
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
 * @param {*} props.updateWidgetSize - Live size updater.
 * @param {*} props.commitWidgetSize - Commits a live size update.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @param {string} [props.title] - Section heading title. Defaults to the translated "Icon".
 * @param {boolean} [props.showUnitsToggle=false] - Whether to show the units toggle.
 * @param {*} props.unitsField - Value for units field.
 * @returns {JSX.Element} Rendered component output.
 */
export function IconSection({
  widget,
  updateWidgetData,
  updateWidgetSize,
  commitWidgetSize,
  setNumericField,
  title,
  showUnitsToggle = false,
  unitsField = null,
}) {
  const { t } = useTranslation()
  const iconSize = widget.data.icon_size
  return (
    <div className="space-y-4">
      <div className="flex w-full items-center gap-3">
        <SectionHeading icon={Palette} title={title} titleKey="widget-editor.icon" defaultTitle="Icon" />
        <div className="shrink-0 pt-1">
          <ToggleField checked={widget.data.show_icon} onCheckedChange={(checked) => updateWidgetData(widget.id, { show_icon: checked })} />
        </div>
      </div>
      <div className="grid grid-cols-2 gap-4">
        <ColorField
          disabled={!widget.data.show_icon}
          value={widget.data.icon_color || getThemeColor('aqua')}
          onChange={(value) => updateWidgetData(widget.id, { icon_color: value })}
        />
        <SizeSlider
          label={t('widget-editor.size', 'Size')}
          value={iconSize}
          disabled={!widget.data.show_icon}
          min={0}
          max={100}
          step={1}
          valueDisplay={`${iconSize}px`}
          onChange={(value) => updateWidgetSize(widget.id, { icon_size: value })}
          onCommit={() => commitWidgetSize(widget.id)}
        />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <NumberField
          disabled={!widget.data.show_icon}
          label={t('widget-editor.horizontalOffset', 'Horizontal Offset')}
          value={widget.data.icon_offset_x}
          onChange={(rawValue) => setNumericField(widget.id, 'icon_offset_x', rawValue)}
        />
        <NumberField
          disabled={!widget.data.show_icon}
          label={t('widget-editor.verticalOffset', 'Vertical Offset')}
          value={widget.data.icon_offset_y}
          onChange={(rawValue) => setNumericField(widget.id, 'icon_offset_y', rawValue)}
        />
      </div>

      {showUnitsToggle && unitsField ? (
        unitsField
      ) : showUnitsToggle ? (
        <ToggleField
          label={t('widget-editor.displayUnits', 'Display Units')}
          checked={widget.data.show_units}
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
 * @param {string} [props.title] - Section heading title. Defaults to the translated "Unit".
 * @param {boolean} [props.showToggle=true] - Whether to show the toggle.
 * @param {string} [props.value] - Current select field value.
 * @param {Function} [props.onValueChange] - Callback for select changes.
 * @param {Array} [props.options] - Select field options.
 * @param {string} [props.selectLabel] - Select field label. Defaults to the translated "Unit".
 * @param {string} [props.colorValue] - Current color picker value.
 * @param {Function} [props.onColorChange] - Callback for color changes.
 * @param {string} [props.colorLabel] - Color field label. Defaults to the translated "Color".
 * @returns {JSX.Element} Rendered component output.
 */
export function UnitsControlRow({
  checked,
  onCheckedChange,
  title,
  showToggle = true,
  value,
  onValueChange,
  options,
  selectLabel,
  colorValue,
  onColorChange,
  colorLabel,
}) {
  const { t } = useTranslation()
  const showSelect = Array.isArray(options) && options.length > 0 && value !== undefined && typeof onValueChange === 'function'
  const showColor = colorValue !== undefined && typeof onColorChange === 'function'
  const controlsDisabled = showToggle && !checked

  return (
    <div className="space-y-2 pt-2">
      <div className="flex w-full items-center gap-3">
        <SectionHeading icon={Ruler} title={title} titleKey="widget-editor.unit" defaultTitle="Unit" />
        {showToggle ? (
          <div className="shrink-0 pt-1">
            <ToggleField checked={checked} onCheckedChange={onCheckedChange} />
          </div>
        ) : null}
      </div>
      {showSelect || showColor ? (
        <div className="grid grid-cols-2 gap-4 items-start">
          {showColor ? <ColorField label={colorLabel} value={colorValue} onChange={onColorChange} disabled={controlsDisabled} /> : null}
          {showSelect ? (
            <SelectField
              label={selectLabel ?? t('widget-editor.unit', 'Unit')}
              value={value}
              onValueChange={onValueChange}
              options={options}
              disabled={controlsDisabled}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  )
}
