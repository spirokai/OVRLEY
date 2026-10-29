import { useMemo } from 'react'
import { ArrowUpDown, Mountain, Ruler, SquareChevronLeft, Type } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import { SectionHeading } from '@/components/ui/section-heading'
import { useAvailableFonts } from '@/hooks/useFonts'
import { DISPLAY_TYPE_DEFINITIONS, WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { buildMetricUnitUpdate } from '@/lib/widget/altitude'
import { useTranslation } from 'react-i18next'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { ColorField, FieldBlock, SelectField, SizeSlider, SliderField, ToggleField } from '../widgetFormControls'

const ORIENTATION_OPTIONS = [
  { value: 'left', labelKey: 'widget-editor.left', defaultLabel: 'Left' },
  { value: 'right', labelKey: 'widget-editor.right', defaultLabel: 'Right' },
]
const MAJOR_TICK_INTERVALS = DISPLAY_TYPE_DEFINITIONS.altitude_tape.majorTickIntervals
const VERTICAL_SPEED_UNIT_OPTIONS = WIDGET_TYPE_DEFINITIONS.vertical_speed.supportedDisplayUnits
const FONT_TRIGGER_CLASS = 'h-9 border-border/70 bg-surface text-xs'
const FONT_LABEL_CLASS = 'text-[0.667rem] text-muted-foreground uppercase font-bold'

/**
 * Editor controls for the vertical altitude or depth tape with live readout and vertical-speed trend.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.updateWidgetSize - Updates size-affecting widget data.
 * @param {Function} props.commitWidgetSize - Commits a size-affecting edit.
 * @returns {JSX.Element}
 */
export default function AltitudeTapeDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const { t } = useTranslation()
  const tape = useMemo(() => widget.data.display_variants?.altitude_tape ?? {}, [widget.data.display_variants?.altitude_tape])
  const updateTape = useDisplayVariantUpdater(widget, 'altitude_tape', updateWidgetData)
  const updateTapeSize = useDisplayVariantUpdater(widget, 'altitude_tape', updateWidgetSize)
  const availableFonts = useAvailableFonts()
  const commit = () => commitWidgetSize(widget.id)
  const unit = widget.data.display_unit
  const showSpeed = tape.show_vertical_speed

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={Mountain} title={t('widget-editor.tapeScale', 'Tape Scale')} />
        <div className="grid grid-cols-2 gap-4">
          <SizeSlider
            label={t('widget-editor.width', 'Width')}
            value={tape.width}
            min={40}
            max={600}
            valueDisplay={`${Math.round(tape.width)}`}
            suffix="px"
            onChange={(width) => updateTapeSize({ width })}
            onCommit={commit}
          />
          <SizeSlider
            label={t('widget-editor.height', 'Height')}
            value={tape.height}
            min={120}
            max={2000}
            valueDisplay={`${Math.round(tape.height)}`}
            suffix="px"
            onChange={(height) => updateTapeSize({ height })}
            onCommit={commit}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label={t('widget-editor.majorInterval', 'Interval')}
            value={MAJOR_TICK_INTERVALS.indexOf(tape.major_tick_interval)}
            min={0}
            max={MAJOR_TICK_INTERVALS.length - 1}
            step={1}
            valueDisplay={`${tape.major_tick_interval} ${unit}`}
            onSliderChange={(index) => updateTape({ major_tick_interval: MAJOR_TICK_INTERVALS[index] })}
          />
          <SliderField
            editable
            label={t('widget-editor.majorSpacing', 'Spacing')}
            value={tape.major_tick_spacing}
            min={20}
            max={300}
            step={1}
            integerDisplay
            suffix="px"
            onSliderChange={(major_tick_spacing) => updateTape({ major_tick_spacing })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <FieldBlock label={t('widget-editor.orientation', 'Orientation')}>
            <ToggleGroup
              type="single"
              className="w-full"
              value={tape.tape_orientation}
              onValueChange={(tape_orientation) => {
                if (tape_orientation) updateTape({ tape_orientation })
              }}
            >
              {ORIENTATION_OPTIONS.map((option) => (
                <ToggleGroupItem key={option.value} value={option.value} className="h-7 w-full px-3 text-xs">
                  {t(option.labelKey, option.defaultLabel)}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </FieldBlock>
          <SelectField
            label={t('widget-editor.unit', 'Unit')}
            value={unit}
            options={WIDGET_TYPE_DEFINITIONS[widget.type].supportedDisplayUnits}
            onValueChange={(displayUnit) =>
              updateWidgetData(widget.id, buildMetricUnitUpdate(widget.type, widget.data.starting_altitude, unit, displayUnit))
            }
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Ruler} title={t('widget-editor.ticks', 'Ticks')} />
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.majorLength', 'Major Length')}
            value={tape.major_tick_length_pct}
            min={5}
            max={100}
            step={1}
            integerDisplay
            suffix="%"
            onSliderChange={(major_tick_length_pct) => updateTape({ major_tick_length_pct })}
          />
          <SliderField
            editable
            label={t('widget-editor.minorLength', 'Minor Length')}
            value={tape.minor_tick_length_pct}
            min={5}
            max={100}
            step={1}
            integerDisplay
            suffix="%"
            onSliderChange={(minor_tick_length_pct) => updateTape({ minor_tick_length_pct })}
          />
          <SliderField
            editable
            label={t('widget-editor.majorThickness', 'Major Thickness')}
            value={tape.major_tick_thickness}
            min={0.5}
            max={8}
            step={0.5}
            valueDisplay={`${tape.major_tick_thickness}`}
            suffix="px"
            onSliderChange={(major_tick_thickness) => updateTape({ major_tick_thickness })}
          />
          <SliderField
            editable
            label={t('widget-editor.minorThickness', 'Minor Thickness')}
            value={tape.minor_tick_thickness}
            min={0.5}
            max={8}
            step={0.5}
            valueDisplay={`${tape.minor_tick_thickness}`}
            suffix="px"
            onSliderChange={(minor_tick_thickness) => updateTape({ minor_tick_thickness })}
          />
          <ColorField
            label={t('widget-editor.majorTickColor', 'Major Color')}
            value={tape.major_tick_color}
            onChange={(major_tick_color) => updateTape({ major_tick_color })}
          />
          <ColorField
            label={t('widget-editor.minorTickColor', 'Minor Color')}
            value={tape.minor_tick_color}
            onChange={(minor_tick_color) => updateTape({ minor_tick_color })}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading
          icon={Type}
          title={t('widget-editor.labels', 'Labels')}
          trailing={
            <ToggleField
              label={t('widget-editor.showLabels', 'Show Labels')}
              checked={tape.show_tick_labels}
              onCheckedChange={(show_tick_labels) => updateTape({ show_tick_labels })}
            />
          }
        />
        <FontSelectField
          label={t('widget-editor.labelFont', 'Label Font')}
          value={tape.label_font}
          onValueChange={(label_font) => updateTape({ label_font })}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName={FONT_TRIGGER_CLASS}
          labelClassName={FONT_LABEL_CLASS}
        />
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.fontSize', 'Font Size')}
            value={tape.label_font_size}
            min={6}
            max={48}
            step={1}
            integerDisplay
            suffix="px"
            onSliderChange={(label_font_size) => updateTape({ label_font_size })}
          />
          <ColorField
            label={t('widget-editor.labelColor', 'Label Color')}
            value={tape.label_color}
            onChange={(label_color) => updateTape({ label_color })}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading
          icon={SquareChevronLeft}
          title={t('widget-editor.liveValue', 'Value')}
          trailing={
            <ToggleField
              label={t('widget-editor.decimal', 'Decimal')}
              checked={tape.show_value_decimal}
              onCheckedChange={(show_value_decimal) => updateTape({ show_value_decimal })}
            />
          }
        />
        <FontSelectField
          label={t('widget-editor.font', 'Font')}
          value={widget.data.font}
          onValueChange={(font) => updateWidgetData(widget.id, { font })}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName={FONT_TRIGGER_CLASS}
          labelClassName={FONT_LABEL_CLASS}
        />
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.fontSize', 'Font Size')}
            value={widget.data.font_size}
            min={8}
            max={120}
            step={1}
            integerDisplay
            suffix="px"
            onSliderChange={(font_size) => updateWidgetData(widget.id, { font_size })}
          />
          <ColorField
            label={t('widget-editor.fontColor', 'Font Color')}
            value={widget.data.color}
            onChange={(color) => updateWidgetData(widget.id, { color })}
          />
          <ColorField
            label={t('widget-editor.borderColor', 'Border Color')}
            value={tape.value_border_color}
            onChange={(value_border_color) => updateTape({ value_border_color })}
          />
          <ColorField
            label={t('widget-editor.backgroundColor', 'Background')}
            value={tape.value_background_color}
            onChange={(value_background_color) => updateTape({ value_background_color })}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading
          icon={ArrowUpDown}
          title={t('widget-editor.verticalSpeed', 'Vert. Speed')}
          trailing={
            <ToggleField
              label={t('widget-editor.verticalSpeed', 'Vert. Speed')}
              checked={tape.show_vertical_speed}
              onCheckedChange={(show_vertical_speed) => updateTape({ show_vertical_speed })}
            />
          }
        />
        <div className="grid grid-cols-2 gap-4">
          <SelectField
            label={t('widget-editor.unit', 'Unit')}
            value={tape.vertical_speed_unit}
            options={VERTICAL_SPEED_UNIT_OPTIONS}
            disabled={!showSpeed}
            onValueChange={(vertical_speed_unit) => updateTape({ vertical_speed_unit })}
          />
          <div className="flex items-center justify-between gap-2 self-end px-1 pb-2">
            <span className="text-[0.667rem] text-muted-foreground uppercase font-bold">{t('widget-editor.decimal', 'Decimal')}</span>
            <ToggleField
              checked={tape.show_vertical_speed_decimal}
              disabled={!showSpeed}
              onCheckedChange={(show_vertical_speed_decimal) => updateTape({ show_vertical_speed_decimal })}
            />
          </div>
          <SliderField
            editable
            label={t('widget-editor.trendSeconds', 'Trend')}
            value={tape.vertical_speed_trend_seconds}
            min={1}
            max={60}
            step={1}
            integerDisplay
            suffix="s"
            disabled={!showSpeed}
            onSliderChange={(vertical_speed_trend_seconds) => updateTape({ vertical_speed_trend_seconds })}
          />
          <SliderField
            editable
            label={t('widget-editor.thickness', 'Thickness')}
            value={tape.vertical_speed_bar_thickness}
            min={1}
            max={20}
            step={1}
            integerDisplay
            suffix="px"
            disabled={!showSpeed}
            onSliderChange={(vertical_speed_bar_thickness) => updateTape({ vertical_speed_bar_thickness })}
          />
          <SliderField
            editable
            label={t('widget-editor.fontSize', 'Font Size')}
            value={tape.vertical_speed_font_size}
            min={6}
            max={48}
            step={1}
            integerDisplay
            suffix="px"
            disabled={!showSpeed}
            onSliderChange={(vertical_speed_font_size) => updateTape({ vertical_speed_font_size })}
          />
          <div />
          <ColorField
            label={t('widget-editor.climbColor', 'Climb')}
            value={tape.vertical_speed_positive_color}
            disabled={!showSpeed}
            onChange={(vertical_speed_positive_color) => updateTape({ vertical_speed_positive_color })}
          />
          <ColorField
            label={t('widget-editor.descentColor', 'Descent')}
            disabled={!showSpeed}
            value={tape.vertical_speed_negative_color}
            onChange={(vertical_speed_negative_color) => updateTape({ vertical_speed_negative_color })}
          />
        </div>
      </div>
    </>
  )
}
