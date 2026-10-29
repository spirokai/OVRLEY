/**
 * Shared section components used across widget editors.
 * Each section is a reusable UI block (Position, Dimensions, Font, Icon, Units).
 */

import { Italic, Move, Palette, Ruler, TrendingUp, Type } from 'lucide-react'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { SectionHeading } from '@/components/ui/section-heading'
import { ColorField, ContentAlignmentControl, NumberField, SelectField, SliderField, TextField, ToggleField } from './widgetFormControls'
import FontSelectField from '@/components/ui/font-select-field'
import { useAvailableFonts, useLabelTypography } from '@/hooks/useFonts'
import { getThemeColor } from '@/lib/color-utils'
import { useTranslation } from 'react-i18next'

/**
 * Renders the position section component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @param {Function} props.updateWidgetDraft - Applies a live widget draft, published once per frame.
 * @param {Function} props.commitWidgetDraft - Commits the live widget draft.
 * @returns {JSX.Element} Rendered component output.
 */
export function PositionSection({ widget, setNumericField, updateWidgetDraft, commitWidgetDraft, headerAction = null }) {
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
        editable
        label={t('widget-editor.transparency', 'Transparency')}
        value={opacity}
        min={0}
        max={100}
        step={1}
        valueDisplay={`${opacity}`}
        suffix="%"
        onSliderChange={(value) => updateWidgetDraft(widget.id, { opacity: value / 100 })}
        onSliderCommit={() => commitWidgetDraft(widget.id)}
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
export function DimensionsSection({ widget, setNumericField, min = 0 }) {
  const { t } = useTranslation()
  return (
    <div className="space-y-3">
      <SectionHeading icon={TrendingUp} title={t('widget-editor.dimensions', 'Dimensions')} />
      <div className="grid grid-cols-2 gap-4">
        <NumberField
          label={t('widget-editor.width', 'Width')}
          value={widget.data.width}
          onChange={(rawValue) => setNumericField(widget.id, 'width', rawValue, { min })}
        />
        <NumberField
          label={t('widget-editor.height', 'Height')}
          value={widget.data.height}
          onChange={(rawValue) => setNumericField(widget.id, 'height', rawValue, { min })}
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
 * @param {*} props.updateWidgetDraft - Applies a live widget draft, published once per frame.
 * @param {*} props.commitWidgetDraft - Commits the live widget draft.
 * @param {string} [props.title] - Section heading title. Defaults to the translated "Typography".
 * @param {boolean} [props.showTextInput=false] - Whether to show the text input.
 * @param {string} [props.fontSizeLabel] - Label for the font size slider. Defaults to the translated "Font Size".
 * @param {number} [props.sizeMin=20] - Minimum font size.
 * @param {number} [props.sizeMax=200] - Maximum font size.
 * @param {string} [props.colorLabel] - Label for the font color picker. Defaults to the translated "Font Color".
 * @param {boolean} [props.showContentAlignment=false] - Whether to show the content alignment control in the section heading.
 * @returns {JSX.Element} Rendered component output.
 */
export function FontSection({
  widget,
  updateWidgetData,
  updateWidgetDraft,
  commitWidgetDraft,
  title,
  showTextInput = false,
  fontSizeLabel,
  sizeMin = 20,
  sizeMax = 200,
  colorLabel,
  showContentAlignment = false,
}) {
  const { t } = useTranslation()
  const fontSize = widget.data.font_size
  const availableFonts = useAvailableFonts()
  const typography = useLabelTypography(widget, updateWidgetData)

  return (
    <div className="space-y-4">
      <SectionHeading
        icon={Type}
        title={title}
        titleKey="widget-editor.typography"
        defaultTitle="Typography"
        trailing={
          widget.category === 'labels' ? (
            <ToggleGroup size="compact" type="multiple" value={typography.italic ? ['italic'] : []} onValueChange={typography.changeItalic}>
              <ToggleGroupItem
                value="italic"
                size="compact"
                disabled={!typography.italicSupported}
                aria-label={t('widget-editor.italic', 'Italic')}
                title={t('widget-editor.italic', 'Italic')}
              >
                <Italic />
              </ToggleGroupItem>
            </ToggleGroup>
          ) : showContentAlignment ? (
            <ContentAlignmentControl
              value={widget.data.content_alignment}
              onValueChange={(value) => updateWidgetData(widget.id, { content_alignment: value })}
            />
          ) : null
        }
      />

      {showTextInput ? (
        <TextField
          label={t('widget-editor.text', 'Text')}
          value={widget.data.text || ''}
          onChange={(value) => updateWidgetData(widget.id, { text: value })}
        />
      ) : null}

      <SliderField
        editable
        integerDisplay
        label={fontSizeLabel ?? t('widget-editor.fontSize', 'Font Size')}
        value={fontSize}
        min={sizeMin}
        max={sizeMax}
        step={1}
        valueDisplay={`${fontSize}`}
        suffix="px"
        onSliderChange={(value) => updateWidgetDraft(widget.id, { font_size: value })}
        onSliderCommit={() => commitWidgetDraft(widget.id)}
      />
      <div className="grid grid-cols-2 gap-4 items-end">
        <FontSelectField
          label={t('widget-editor.fontFamily', 'Font Family')}
          value={widget.data.font}
          onValueChange={typography.changeFont}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName="h-9 border-border/70 bg-surface text-xs"
          labelClassName="text-[0.667rem] text-muted-foreground uppercase font-bold"
        />
        <ColorField
          label={colorLabel}
          labelKey="widget-editor.fontColor"
          defaultLabel="Font Color"
          value={widget.data.color || getThemeColor('ice')}
          onChange={(value) => updateWidgetData(widget.id, { color: value })}
        />
      </div>
      {widget.category === 'labels' ? (
        <div className="grid grid-cols-2 gap-4 items-end pt-2">
          {typography.font ? (
            <SliderField
              editable
              integerDisplay
              label={t('widget-editor.fontWeight', 'Font Weight')}
              value={typography.weight}
              min={typography.weightAxis?.min ?? 1}
              max={typography.weightAxis?.max ?? 1000}
              disabled={!typography.weightAxis}
              step={100}
              valueDisplay={String(typography.weight)}
              onSliderChange={(value) => updateWidgetDraft(widget.id, { font_weight: value })}
              onSliderCommit={() => commitWidgetDraft(widget.id)}
            />
          ) : null}
          <SliderField
            editable
            label={t('widget-editor.letterSpacing', 'Letter Spacing')}
            value={widget.data.letter_spacing}
            min={-35}
            max={35}
            step={0.5}
            valueDisplay={`${widget.data.letter_spacing.toFixed(1)}`}
            suffix="%"
            onSliderChange={(value) => updateWidgetDraft(widget.id, { letter_spacing: value })}
            onSliderCommit={() => commitWidgetDraft(widget.id)}
          />
        </div>
      ) : null}
    </div>
  )
}

/**
 * Renders the icon section component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.updateWidgetDraft - Applies a live widget draft, published once per frame.
 * @param {*} props.commitWidgetDraft - Commits the live widget draft.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @param {string} [props.title] - Section heading title. Defaults to the translated "Icon".
 * @param {boolean} [props.showUnitsToggle=false] - Whether to show the units toggle.
 * @param {*} props.unitsField - Value for units field.
 * @returns {JSX.Element} Rendered component output.
 */
export function IconSection({
  widget,
  updateWidgetData,
  updateWidgetDraft,
  commitWidgetDraft,
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
        <SliderField
          editable
          integerDisplay
          label={t('widget-editor.size', 'Size')}
          value={iconSize}
          disabled={!widget.data.show_icon}
          min={0}
          max={100}
          step={1}
          valueDisplay={`${iconSize}`}
          suffix="px"
          onSliderChange={(value) => updateWidgetDraft(widget.id, { icon_size: value })}
          onSliderCommit={() => commitWidgetDraft(widget.id)}
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
