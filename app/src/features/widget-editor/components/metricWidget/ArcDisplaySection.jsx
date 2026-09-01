import { useMemo } from 'react'
import { SlidersHorizontal, Tags } from 'lucide-react'
import { buildUniformResizeUpdate } from '@/features/overlay-editor/utils/widgetResizeScaling'
import {
  getStandardMetricDefinition,
  getStandardMetricDisplayUnit,
  getStandardMetricUnitOptions,
  getStandardMetricUnitsMode,
} from '@/lib/widget/standard-metrics'
import FontSelectField from '@/components/ui/font-select-field'
import { SectionHeading } from '@/components/ui/section-heading'
import useAvailableFonts from '@/features/scene-settings/hooks/useAvailableFonts'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { FontSection, UnitsControlRow } from '../widgetEditorSections'
import { ColorField, SelectField, SizeSlider, SliderField, ToggleField } from '../widgetFormControls'
import { BarFillStyleDetails, BarFillStyleField } from './BarFillStyleControls'
import { getArcGaugeLayout, getCornerGaugeLayout } from '@/features/widget-preview/widgets/arc-gauge/geometry'
import { getArcBarGapMax, getArcTrackCornerRadiusMax, getSuggestedArcBarGeometry } from '@/features/widget-preview/shared/gaugeBarGeometry'
import { buildMetricUnitUpdate } from '@/lib/widget/altitude'
import { TYPE_DEFAULTS } from '@/lib/widget/standard-widgets'
import { useTranslation } from 'react-i18next'

const ARC_MIN_ANGLE = 30
const ARC_MAX_ANGLE = 360
const CORNER_ORIENTATION_OPTIONS = [
  { value: 'bottom-left', label: 'Bottom Left' },
  { value: 'bottom-right', label: 'Bottom Right' },
]

function suggestArcBarGeometry(data) {
  const isCorner = data.corner_orientation != null
  const layout = isCorner ? getCornerGaugeLayout(data, null, []) : getArcGaugeLayout(data, null, [])
  return getSuggestedArcBarGeometry({ ...layout, corner: isCorner, borderThickness: data.track_border_thickness })
}

function getArcGapMax(data) {
  const layout = data.corner_orientation == null ? getArcGaugeLayout(data, null, []) : getCornerGaugeLayout(data, null, [])
  return getArcBarGapMax({ ...layout, bar_count: data.bar_count })
}

function getArcCornerRadiusMax(data) {
  const layout = data.corner_orientation == null ? getArcGaugeLayout(data, null, []) : getCornerGaugeLayout(data, null, [])
  return getArcTrackCornerRadiusMax({ ...data, ...layout })
}

/**
 * Arc-shaped gauge controls. Each display type owns its own variant data while
 * sharing track and inner-widget styling. The value/unit typography remains
 * shared top-level metric data. Icons are deliberately absent because gauges do not
 * render them.
 */
export default function ArcDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const { t } = useTranslation()
  const displayType = widget.data.display_type
  const isCornerGauge = displayType === 'corner'
  const arcData = useMemo(() => widget.data.display_variants?.[displayType] ?? {}, [displayType, widget.data.display_variants])
  const updateArc = useDisplayVariantUpdater(widget, displayType, arcData, updateWidgetData)
  const updateArcSize = useDisplayVariantUpdater(widget, displayType, arcData, updateWidgetSize)
  const availableFonts = useAvailableFonts()
  const definition = getStandardMetricDefinition(widget.type)
  const unitOptions = getStandardMetricUnitOptions(widget.type)
  const unitsMode = getStandardMetricUnitsMode(widget.type)
  const supportsUnitSelection = unitOptions.length > 1
  const cornerRadiusMax = getArcCornerRadiusMax(arcData)
  const size = widget.data.width ?? arcData.width
  const barGapMax = arcData.track_fill_style === 'bars' ? getArcGapMax(arcData) : 0
  const hasDecimalControl = definition?.maxDecimals !== undefined || definition?.formatter === 'decimal' || definition?.formatter === 'temperature'
  const maxDecimals = definition?.maxDecimals ?? 1
  const defaultDecimals = TYPE_DEFAULTS[widget.type]?.decimals ?? 1
  const decimals = Number.isFinite(widget.data.decimals) ? Math.min(Math.max(widget.data.decimals, 0), maxDecimals) : defaultDecimals

  const updateBoundedNumber = (updateVariant, key, rawValue, min, max) => {
    const value = Number(rawValue)
    if (!Number.isFinite(value)) return
    updateVariant({ [key]: Math.min(max, Math.max(min, value)) })
  }

  const handleSizeChange = (nextSize) => {
    const update = buildUniformResizeUpdate(widget, nextSize)
    if (update) updateWidgetSize(widget.id, update)
  }

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={SlidersHorizontal} title={isCornerGauge ? 'Corner Track' : t('widget-editor.arcTrack', 'Arc Track')} />
        <div className="grid grid-cols-1 gap-4">
          <SizeSlider
            label="Size"
            value={size}
            min={30}
            max={600}
            step={1}
            valueDisplay={`${Math.round(size)}px`}
            onChange={handleSizeChange}
            onCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4 pt-2">
          {isCornerGauge ? (
            <SelectField
              label={t('widget-editor.cornerOrientation', 'Corner Orientation')}
              value={arcData.corner_orientation}
              onValueChange={(corner_orientation) => updateArc({ corner_orientation })}
              options={CORNER_ORIENTATION_OPTIONS}
            />
          ) : (
            <SliderField
              label={t('widget-editor.arcAngle', 'Arc Angle')}
              value={arcData.arc_angle}
              min={ARC_MIN_ANGLE}
              max={ARC_MAX_ANGLE}
              step={5}
              valueDisplay={`${arcData.arc_angle}°`}
              onSliderChange={(arc_angle) => updateArcSize({ arc_angle })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          )}
          <SliderField
            label="Thickness"
            value={arcData.track_thickness}
            min={1}
            max={100}
            step={1}
            integerDisplay
            valueDisplay={`${arcData.track_thickness}px`}
            onSliderChange={(track_thickness) =>
              updateArcSize({
                track_thickness,
                track_corner_radius: Math.min(arcData.track_corner_radius, getArcCornerRadiusMax({ ...arcData, track_thickness })),
              })
            }
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <BarFillStyleField data={arcData} suggestBarGeometry={suggestArcBarGeometry} updateVariant={updateArc} />
          <SliderField
            label={t('widget-editor.cornerRadius', 'Corner Radius')}
            value={arcData.track_corner_radius}
            min={0}
            max={cornerRadiusMax}
            step={1}
            integerDisplay
            valueDisplay={`${arcData.track_corner_radius}px`}
            onSliderChange={(track_corner_radius) => updateArcSize({ track_corner_radius })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
          <BarFillStyleDetails
            data={arcData}
            barGapMax={barGapMax}
            getCornerRadiusMax={getArcCornerRadiusMax}
            updateVariant={updateArc}
            updateVariantSize={updateArcSize}
            commitWidgetSize={commitWidgetSize}
            widgetId={widget.id}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField label={t('widget-editor.borderColor', 'Border Color')} value={arcData.track_border_color} onChange={(track_border_color) => updateArc({ track_border_color })} />
          <SliderField
            label="Border"
            value={arcData.track_border_thickness}
            min={0}
            max={6}
            step={1}
            integerDisplay
            valueDisplay={`${arcData.track_border_thickness}px`}
            onSliderChange={(track_border_thickness) => updateArcSize({ track_border_thickness })}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField label={t('widget-editor.emptyColor', 'Empty Color')} value={arcData.track_empty_color} onChange={(track_empty_color) => updateArc({ track_empty_color })} />
          <SliderField
            label={t('widget-editor.emptyOpacity', 'Empty Opacity')}
            value={arcData.track_empty_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round((arcData.track_empty_opacity ?? 0) * 100)}%`}
            onSliderChange={(track_empty_opacity) => updateArc({ track_empty_opacity })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField label={t('widget-editor.filledColor', 'Filled Color')} value={arcData.track_filled_color} onChange={(track_filled_color) => updateArc({ track_filled_color })} />
          <SliderField
            label={t('widget-editor.filledOpacity', 'Filled Opacity')}
            value={arcData.track_filled_opacity}
            min={0}
            max={1}
            step={0.05}
            valueDisplay={`${Math.round((arcData.track_filled_opacity ?? 0) * 100)}%`}
            onSliderChange={(track_filled_opacity) => updateArc({ track_filled_opacity })}
          />
        </div>
      </div>

      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        title="Label"
        fontSizeLabel="Font Size"
      />
      <div className="grid grid-cols-2 gap-4">
        <SliderField
          label={t('widget-editor.horizontalOffset', 'Horizontal Offset')}
          value={arcData.inner_widget_offset_x}
          min={-50}
          max={50}
          step={1}
          integerDisplay
          valueDisplay={`${arcData.inner_widget_offset_x}px`}
          onSliderChange={(value) => updateBoundedNumber(updateArcSize, 'inner_widget_offset_x', value, -10_000, 10_000)}
          onSliderCommit={() => commitWidgetSize(widget.id)}
        />
        <SliderField
          label={t('widget-editor.verticalOffset', 'Vertical Offset')}
          value={arcData.inner_widget_offset_y}
          min={-50}
          max={50}
          step={1}
          integerDisplay
          valueDisplay={`${arcData.inner_widget_offset_y}px`}
          onSliderChange={(value) => updateBoundedNumber(updateArcSize, 'inner_widget_offset_y', value, -10_000, 10_000)}
          onSliderCommit={() => commitWidgetSize(widget.id)}
        />
      </div>
      {hasDecimalControl ? (
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label="Decimals"
            value={decimals}
            min={0}
            max={maxDecimals}
            step={1}
            valueDisplay={String(decimals)}
            onSliderChange={(value) => updateWidgetData(widget.id, { decimals: value })}
          />
        </div>
      ) : null}

      {unitsMode !== 'hidden' ? (
        <UnitsControlRow
          title="Unit"
          checked={widget.data.show_units ?? definition?.showUnitsByDefault ?? false}
          onCheckedChange={(show_units) => updateWidgetData(widget.id, { show_units })}
          colorValue={widget.data.unit_color}
          onColorChange={(unit_color) => updateWidgetData(widget.id, { unit_color })}
          selectLabel="Unit"
          value={getStandardMetricDisplayUnit(widget.type, widget.data)}
          onValueChange={(displayUnit) =>
            updateWidgetData(
              widget.id,
              buildMetricUnitUpdate(widget.type, widget.data.starting_altitude, getStandardMetricDisplayUnit(widget.type, widget.data), displayUnit),
            )
          }
          options={supportsUnitSelection ? unitOptions : undefined}
        />
      ) : null}

      <div className="space-y-4">
        <div className="flex w-full items-center gap-3">
          <SectionHeading icon={Tags} title={t('widget-editor.minmaxLabels', 'Min/Max Labels')} />
          <div className="shrink-0 pt-1">
            <ToggleField checked={arcData.show_min_max_labels} onCheckedChange={(show_min_max_labels) => updateArc({ show_min_max_labels })} />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-4 pt-2">
          <FontSelectField
            label={t('widget-editor.labelFont', 'Label Font')}
            value={arcData.min_max_label_font}
            disabled={!arcData.show_min_max_labels}
            onValueChange={(min_max_label_font) => updateArc({ min_max_label_font })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName="h-9 border-border/70 bg-surface text-xs"
            labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
          />
          <SizeSlider
            label={t('widget-editor.fontSize', 'Font Size')}
            disabled={!arcData.show_min_max_labels}
            value={arcData.min_max_label_font_size}
            min={6}
            max={50}
            step={1}
            valueDisplay={`${arcData.min_max_label_font_size}px`}
            onChange={(min_max_label_font_size) => updateArcSize({ min_max_label_font_size })}
            onCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <ColorField
          label={t('widget-editor.labelColor', 'Label Color')}
          value={arcData.min_max_label_color}
          disabled={!arcData.show_min_max_labels}
          onChange={(min_max_label_color) => updateArc({ min_max_label_color })}
        />
      </div>
    </>
  )
}
