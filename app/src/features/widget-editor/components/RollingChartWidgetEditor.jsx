/**
 * Editor for the time-rolling chart: plotted metrics, line styling, marker, and legend.
 */

import { ChartLine, ChevronDown, ChevronUp, Palette, Plus, Trash2, Type } from 'lucide-react'
import { Button } from '@/components/ui/button'
import FontSelectField from '@/components/ui/font-select-field'
import HexColorPicker from '@/components/ui/hex-color-picker'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { SectionHeading } from '@/components/ui/section-heading'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useAvailableFonts } from '@/hooks/useFonts'
import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { useTranslation } from 'react-i18next'
import { useWindowSecondsInput } from '../hooks/useWindowSecondsInput'
import {
  CHART_LIMITS,
  addChartSeries,
  moveChartSeries,
  recolorChartSeries,
  removeChartSeries,
  replaceChartSeriesMetric,
  unusedChartMetrics,
} from '../utils/rollingChart'
import PlotMarkerSection from './PlotMarkerSection'
import { DimensionsSection } from './widgetEditorSections'
import { CONTROL_CLASS, ColorField, FieldBlock, SliderField, ToggleField } from './widgetFormControls'

const TOGGLE_LABEL_CLASS = 'p-0 text-[9px] text-muted-foreground uppercase font-bold'
const ROW_BUTTON_CLASS = 'text-muted-foreground hover:text-foreground'
const ADD_BUTTON_CLASS = 'shadow-none text-muted-foreground hover:bg-surface-elevated hover:text-foreground [&_svg]:size-[0.9rem]'

/**
 * One plotted metric: its metric, line color, and ordering controls.
 *
 * @param {object} props - Component props.
 * @param {object} props.item - Series row.
 * @param {string[]} props.metrics - Metrics this row may switch to, including its own.
 * @param {boolean} props.first - Whether the row draws furthest back.
 * @param {boolean} props.last - Whether the row draws in front.
 * @param {boolean} props.only - Whether the row is the chart's only metric.
 * @param {Function} props.onMetricChange - Picks another metric.
 * @param {Function} props.onColorChange - Recolors the line.
 * @param {Function} props.onMove - Moves the row by -1 or 1.
 * @param {Function} props.onRemove - Removes the row.
 * @returns {JSX.Element} Rendered row.
 */
function ChartSeriesRow({ item, metrics, first, last, only, onMetricChange, onColorChange, onMove, onRemove }) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-1">
      <Select value={item.metric} onValueChange={onMetricChange}>
        <SelectTrigger className={`${CONTROL_CLASS} min-w-0 flex-1`} aria-label={t('widget-editor.chartMetric', 'Metric')}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {metrics.map((metric) => (
            <SelectItem key={metric} value={metric}>
              {t(WIDGET_TYPE_DEFINITIONS[metric].nameKey)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <HexColorPicker
        value={item.color}
        onChange={onColorChange}
        showValue={false}
        className="shrink-0"
        triggerClassName="w-9 flex-none justify-center px-0"
      />
      <Button
        variant="ghost"
        size="toolbar-icon"
        className={ROW_BUTTON_CLASS}
        disabled={first}
        onClick={() => onMove(-1)}
        aria-label={t('widget-editor.moveUp', 'Move up')}
      >
        <ChevronUp />
      </Button>
      <Button
        variant="ghost"
        size="toolbar-icon"
        className={ROW_BUTTON_CLASS}
        disabled={last}
        onClick={() => onMove(1)}
        aria-label={t('widget-editor.moveDown', 'Move down')}
      >
        <ChevronDown />
      </Button>
      <Button
        variant="ghost"
        size="toolbar-icon"
        className={ROW_BUTTON_CLASS}
        disabled={only}
        onClick={onRemove}
        aria-label={t('widget-editor.removeMetric', 'Remove metric')}
      >
        <Trash2 />
      </Button>
    </div>
  )
}

/**
 * Renders the rolling chart widget editor.
 *
 * @param {object} props - Component props.
 * @param {object} props.widget - Rolling chart widget.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.updateWidgetDraft - Applies a live widget draft, published once per frame.
 * @param {Function} props.commitWidgetDraft - Commits the live widget draft.
 * @param {Function} props.setNumericField - Parses and stores numeric input.
 * @returns {JSX.Element} Rendered component output.
 */
export default function RollingChartWidgetEditor({ widget, updateWidgetData, updateWidgetDraft, commitWidgetDraft, setNumericField }) {
  const { t } = useTranslation()
  const availableFonts = useAvailableFonts()
  const { data } = widget
  const series = data.series
  const unused = unusedChartMetrics(series)
  const setSeries = (next) => updateWidgetData(widget.id, { series: next })
  const commit = () => commitWidgetDraft(widget.id)
  const windowInput = useWindowSecondsInput({
    value: data.window_seconds,
    onCommit: (window_seconds) => updateWidgetData(widget.id, { window_seconds }),
  })

  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} min={8} />
      <FieldBlock label={t('widget-editor.timeWindow', 'Time Window')}>
        <Input {...windowInput} aria-label={t('widget-editor.timeWindow', 'Time Window')} className={CONTROL_CLASS} />
      </FieldBlock>
      <div className="space-y-3">
        <SectionHeading
          icon={ChartLine}
          title={t('widget-editor.chartMetrics', 'Metrics')}
          trailing={
            <Button
              variant="outline"
              size="toolbar-icon"
              className={ADD_BUTTON_CLASS}
              disabled={series.length >= CHART_LIMITS.maxSeries || unused.length === 0}
              onClick={() => setSeries(addChartSeries(series))}
              aria-label={t('widget-editor.addMetric', 'Add metric')}
            >
              <Plus />
            </Button>
          }
        />
        {series.map((item, index) => (
          <ChartSeriesRow
            key={index}
            item={item}
            metrics={[item.metric, ...unused]}
            first={index === 0}
            last={index === series.length - 1}
            only={series.length === 1}
            onMetricChange={(metric) => setSeries(replaceChartSeriesMetric(series, index, metric))}
            onColorChange={(color) => setSeries(recolorChartSeries(series, index, color))}
            onMove={(offset) => setSeries(moveChartSeries(series, index, offset))}
            onRemove={() => setSeries(removeChartSeries(series, index))}
          />
        ))}
      </div>
      <div className="space-y-4">
        <SectionHeading
          icon={Palette}
          title={t('widget-editor.lineStyling', 'Line Styling')}
          trailing={
            <div className="flex items-center gap-1.5">
              <span className={TOGGLE_LABEL_CLASS}>{t('widget-editor.fill', 'Fill')}</span>
              <ToggleField
                label={t('widget-editor.fill', 'Fill')}
                checked={data.area_fill}
                onCheckedChange={(area_fill) => updateWidgetData(widget.id, { area_fill })}
              />
            </div>
          }
        />
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            integerDisplay
            label={t('widget-editor.thickness', 'Thickness')}
            value={data.line_width}
            min={1}
            max={20}
            step={1}
            valueDisplay={`${data.line_width}`}
            suffix="px"
            onSliderChange={(line_width) => updateWidgetDraft(widget.id, { line_width })}
            onSliderCommit={commit}
          />
          <SliderField
            editable
            label={t('widget-editor.opacity', 'Opacity')}
            value={data.line_opacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${data.line_opacity}`}
            suffix="%"
            onSliderChange={(line_opacity) => updateWidgetDraft(widget.id, { line_opacity })}
            onSliderCommit={commit}
          />
        </div>
        <SliderField
          editable
          label={t('widget-editor.smoothing', 'Smoothing')}
          value={data.line_curve}
          min={0}
          max={100}
          step={5}
          valueDisplay={`${data.line_curve}`}
          suffix="%"
          onSliderChange={(line_curve) => updateWidgetDraft(widget.id, { line_curve })}
          onSliderCommit={commit}
        />
        <div className="grid grid-cols-2 items-end gap-4">
          <SliderField
            editable
            label={t('widget-editor.areaOpacity', 'Area Opacity')}
            value={data.area_opacity}
            min={0}
            max={100}
            step={1}
            disabled={!data.area_fill}
            valueDisplay={`${data.area_opacity}`}
            suffix="%"
            onSliderChange={(area_opacity) => updateWidgetDraft(widget.id, { area_opacity })}
            onSliderCommit={commit}
          />
          <div className="flex h-9 items-center justify-between gap-2 px-1">
            <Label className={TOGGLE_LABEL_CLASS} disabled={!data.area_fill}>
              {t('widget-editor.areaGradient', 'Gradient')}
            </Label>
            <ToggleField
              label={t('widget-editor.areaGradient', 'Gradient')}
              checked={data.area_gradient}
              disabled={!data.area_fill}
              onCheckedChange={(area_gradient) => updateWidgetData(widget.id, { area_gradient })}
            />
          </div>
        </div>
      </div>
      <PlotMarkerSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetDraft={updateWidgetDraft}
        commitWidgetDraft={commitWidgetDraft}
        showColor={false}
        disabled={!data.show_marker}
        trailing={
          <ToggleField
            label={t('widget-editor.marker', 'Marker')}
            checked={data.show_marker}
            onCheckedChange={(show_marker) => updateWidgetData(widget.id, { show_marker })}
          />
        }
      />
      <div className="space-y-4">
        <SectionHeading
          icon={Type}
          title={t('widget-editor.legend', 'Legend')}
          trailing={
            <ToggleField
              label={t('widget-editor.legend', 'Legend')}
              checked={data.show_legend}
              onCheckedChange={(show_legend) => updateWidgetData(widget.id, { show_legend })}
            />
          }
        />
        <div className="grid grid-cols-2 items-end gap-4">
          <FontSelectField
            label={t('widget-editor.font', 'Font')}
            value={data.legend_font}
            disabled={!data.show_legend}
            onValueChange={(legend_font) => updateWidgetData(widget.id, { legend_font })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName={CONTROL_CLASS}
            labelClassName={TOGGLE_LABEL_CLASS}
          />
          <SliderField
            editable
            integerDisplay
            label={t('widget-editor.fontSize', 'Font Size')}
            value={data.legend_font_size}
            min={6}
            max={100}
            step={1}
            disabled={!data.show_legend}
            valueDisplay={`${data.legend_font_size}`}
            suffix="px"
            onSliderChange={(legend_font_size) => updateWidgetDraft(widget.id, { legend_font_size })}
            onSliderCommit={commit}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.fontColor', 'Font Color')}
            value={data.legend_color}
            disabled={!data.show_legend}
            onChange={(legend_color) => updateWidgetData(widget.id, { legend_color })}
          />
        </div>
      </div>
    </>
  )
}
