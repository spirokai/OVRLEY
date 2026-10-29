import { ChartNoAxesGantt, Rows3, Type } from 'lucide-react'
import FontSelectField from '@/components/ui/font-select-field'
import GradientPicker from '@/components/ui/gradient-picker'
import { SectionHeading } from '@/components/ui/section-heading'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { useAvailableFonts } from '@/hooks/useFonts'
import { useTranslation } from 'react-i18next'
import {
  SPLIT_COLUMNS,
  SPLIT_UNITS,
  buildSplitBarHeightUpdate,
  buildSplitDistanceUpdate,
  buildVisibleColumnsUpdate,
  maxSplitBarBorderThickness,
  maxSplitBarCornerRadius,
} from '../../utils/splitsRun'
import { FontSection } from '../widgetEditorSections'
import { ColorField, FieldBlock, NumberField, SelectField, SliderField, TextField, ToggleField } from '../widgetFormControls'

const COLUMN_LABELS = {
  split: ['widget-editor.splitColumnSplit', 'Split'],
  bar: ['widget-editor.splitColumnBar', 'Bar'],
  pace: ['widget-editor.splitColumnPace', 'Pace'],
}

/**
 * Renders running splits configuration controls.
 *
 * @param {object} props
 * @param {object} props.widget - Splits widget configuration.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.updateWidgetDraft - Applies a live widget draft, published once per frame.
 * @param {Function} props.commitWidgetDraft - Commits the live widget draft.
 * @returns {JSX.Element} Splits controls.
 */
export default function SplitsRunDisplaySection({ widget, updateWidgetData, updateWidgetDraft, commitWidgetDraft }) {
  const { t } = useTranslation()
  const availableFonts = useAvailableFonts()
  const { data } = widget
  const update = (patch) => updateWidgetData(widget.id, patch)
  const updateDraft = (patch) => updateWidgetDraft(widget.id, patch)
  const commit = () => commitWidgetDraft(widget.id)
  const showBar = data.visible_columns.includes('bar')
  const maxCornerRadius = maxSplitBarCornerRadius(data)
  const maxBorderThickness = maxSplitBarBorderThickness(data)

  return (
    <>
      <FontSection widget={widget} updateWidgetData={updateWidgetData} updateWidgetDraft={updateWidgetDraft} commitWidgetDraft={commitWidgetDraft} />

      <div className="space-y-4">
        <SectionHeading icon={Rows3} title={t('widget-editor.splits', 'Splits')} />
        <div className="grid grid-cols-2 items-end gap-4">
          <NumberField
            label={t('widget-editor.splitLength', 'Split Length')}
            value={data.split_distance}
            min={0}
            step={0.1}
            integerDisplay={false}
            onChange={(raw) => {
              const patch = buildSplitDistanceUpdate(raw)
              if (patch) update(patch)
            }}
          />
          <SelectField
            label={t('widget-editor.unit', 'Unit')}
            value={data.split_unit}
            onValueChange={(split_unit) => update({ split_unit })}
            options={SPLIT_UNITS.map((unit) => ({ value: unit, label: unit }))}
          />
        </div>
        <FieldBlock label={t('widget-editor.splitColumns', 'Columns')}>
          <ToggleGroup
            type="multiple"
            className="w-full"
            value={data.visible_columns}
            onValueChange={(columns) => {
              const patch = buildVisibleColumnsUpdate(columns)
              if (patch) updateDraft(patch)
            }}
          >
            {SPLIT_COLUMNS.map((column) => (
              <ToggleGroupItem key={column} value={column} className="h-7 w-full px-3 text-xs">
                {t(...COLUMN_LABELS[column])}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </FieldBlock>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            integerDisplay
            label={t('widget-editor.maxSplits', 'Max Splits')}
            value={data.max_splits}
            min={1}
            max={30}
            step={1}
            onSliderChange={(max_splits) => updateDraft({ max_splits })}
            onSliderCommit={commit}
          />
          <SliderField
            editable
            integerDisplay
            label={t('widget-editor.rowGap', 'Row Gap')}
            value={data.row_gap}
            min={0}
            max={100}
            valueDisplay={`${Math.round(data.row_gap)}`}
            suffix="px"
            onSliderChange={(row_gap) => updateDraft({ row_gap })}
            onSliderCommit={commit}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Type} title={t('widget-editor.label', 'Label')} />
        <TextField label={t('widget-editor.label', 'Label')} value={data.label} onChange={(label) => update({ label })} />
        <SliderField
          editable
          integerDisplay
          label={t('widget-editor.labelFontSize', 'Label Font Size')}
          value={data.label_font_size}
          min={6}
          max={100}
          step={0.5}
          valueDisplay={`${data.label_font_size}`}
          suffix="px"
          onSliderChange={(label_font_size) => updateDraft({ label_font_size })}
          onSliderCommit={commit}
        />
        <div className="grid grid-cols-2 items-end gap-4">
          <FontSelectField
            label={t('widget-editor.labelFont', 'Label Font')}
            value={data.label_font}
            onValueChange={(label_font) => update({ label_font })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName="h-9 border-border/70 bg-surface text-xs"
            labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
          />
          <ColorField
            label={t('widget-editor.labelColor', 'Label Color')}
            value={data.label_color}
            onChange={(label_color) => update({ label_color })}
          />
        </div>
      </div>

      {showBar ? (
        <div className="space-y-4">
          <SectionHeading
            icon={ChartNoAxesGantt}
            title={t('widget-editor.bar', 'Bar')}
            trailing={
              <div className="flex items-center gap-1.5">
                <span className="text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.fillGradient', 'Gradient')}</span>
                <ToggleField
                  label={t('widget-editor.fillGradient', 'Gradient')}
                  checked={data.track_filled_gradient}
                  onCheckedChange={(track_filled_gradient) => update({ track_filled_gradient })}
                />
              </div>
            }
          />
          <div className="grid grid-cols-2 gap-4">
            <SliderField
              editable
              integerDisplay
              label={t('widget-editor.barWidth', 'Bar Width')}
              value={data.bar_width}
              min={10}
              max={1600}
              valueDisplay={`${Math.round(data.bar_width)}`}
              suffix="px"
              onSliderChange={(bar_width) => updateDraft({ bar_width })}
              onSliderCommit={commit}
            />
            <SliderField
              editable
              integerDisplay
              label={t('widget-editor.barHeight', 'Bar Height')}
              value={data.track_thickness}
              min={2}
              max={200}
              valueDisplay={`${Math.round(data.track_thickness)}`}
              suffix="px"
              onSliderChange={(track_thickness) => updateDraft(buildSplitBarHeightUpdate(data, track_thickness))}
              onSliderCommit={commit}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <SliderField
              editable
              label={t('widget-editor.border', 'Border')}
              value={Math.min(data.track_border_thickness, maxBorderThickness)}
              min={0}
              max={maxBorderThickness}
              step={1}
              integerDisplay
              valueDisplay={`${Math.min(data.track_border_thickness, maxBorderThickness)}`}
              suffix="px"
              onSliderChange={(track_border_thickness) => updateDraft({ track_border_thickness })}
              onSliderCommit={commit}
            />
            <SliderField
              editable
              label={t('widget-editor.cornerRadius', 'Corner Radius')}
              value={Math.min(data.track_corner_radius, maxCornerRadius)}
              min={0}
              max={maxCornerRadius}
              step={1}
              integerDisplay
              suffix="px"
              onSliderChange={(track_corner_radius) => updateDraft({ track_corner_radius })}
              onSliderCommit={commit}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <ColorField
              label={t('widget-editor.filledColor', 'Filled Color')}
              value={data.track_filled_color}
              disabled={data.track_filled_gradient}
              onChange={(track_filled_color) => update({ track_filled_color })}
            />
            <SliderField
              editable
              label={t('widget-editor.filledOpacity', 'Filled Opacity')}
              value={data.track_filled_opacity}
              min={0}
              max={1}
              step={0.05}
              valueScale={100}
              valueDisplay={`${Math.round(data.track_filled_opacity * 100)}`}
              suffix="%"
              onSliderChange={(track_filled_opacity) => updateDraft({ track_filled_opacity })}
              onSliderCommit={commit}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <ColorField
              label={t('widget-editor.borderColor', 'Border Color')}
              value={data.track_border_color}
              onChange={(track_border_color) => update({ track_border_color })}
            />
            <SliderField
              editable
              label={t('widget-editor.borderOpacity', 'Border Opacity')}
              value={data.track_border_opacity}
              min={0}
              max={1}
              step={0.05}
              valueScale={100}
              valueDisplay={`${Math.round(data.track_border_opacity * 100)}`}
              suffix="%"
              onSliderChange={(track_border_opacity) => updateDraft({ track_border_opacity })}
              onSliderCommit={commit}
            />
          </div>
          {data.track_filled_gradient ? (
            <GradientPicker
              stops={data.track_filled_gradient_stops}
              onChange={(track_filled_gradient_stops) => updateDraft({ track_filled_gradient_stops })}
              onCommit={commit}
            />
          ) : null}
        </div>
      ) : null}
    </>
  )
}
