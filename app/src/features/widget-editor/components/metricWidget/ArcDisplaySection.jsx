import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { useMemo } from 'react'
import { SlidersHorizontal, Tags } from 'lucide-react'
import { buildUniformResizeUpdate } from '../../utils/widgetSize'
import FontSelectField from '@/components/ui/font-select-field'
import { SectionHeading } from '@/components/ui/section-heading'
import { useAvailableFonts } from '@/hooks/useFonts'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { FontSection, UnitsControlRow } from '../widgetEditorSections'
import { ColorField, SelectField, SizeSlider, SliderField, ToggleField } from '../widgetFormControls'
import { BarFillStyleDetails, BarFillStyleField } from './BarFillStyleControls'
import TrackStyleSection from './TrackStyleSection'
import { getArcBarGapMax, getArcTrackCornerRadiusMax, getSuggestedArcBarGeometry, getArcTrackDimensions } from '../../utils/gaugeGeometry'
import { buildMetricUnitUpdate } from '@/lib/widget/altitude'
import { useTranslation } from 'react-i18next'
import { translateOptions } from '@/i18n'

const ARC_MIN_ANGLE = 30
const ARC_MAX_ANGLE = 360
const CORNER_ORIENTATION_OPTIONS = [
  { value: 'bottom-left', labelKey: 'widget-editor.bottomLeft', defaultLabel: 'Bottom Left' },
  { value: 'bottom-right', labelKey: 'widget-editor.bottomRight', defaultLabel: 'Bottom Right' },
]

function suggestArcBarGeometry(data, displayType) {
  const isCorner = displayType === 'corner'
  const layout = getArcTrackDimensions(data, displayType)
  return getSuggestedArcBarGeometry({ ...layout, corner: isCorner, borderThickness: data.track_border_thickness })
}

function getArcGapMax(data, displayType) {
  const layout = getArcTrackDimensions(data, displayType)
  return getArcBarGapMax({ ...layout, bar_count: data.bar_count })
}

function getArcCornerRadiusMax(data, displayType) {
  const layout = getArcTrackDimensions(data, displayType)
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
  const isGtGauge = displayType === 'gt_gauge'
  const arcData = useMemo(() => widget.data.display_variants?.[displayType] ?? {}, [displayType, widget.data.display_variants])
  const updateArc = useDisplayVariantUpdater(widget, displayType, updateWidgetData)
  const updateArcSize = useDisplayVariantUpdater(widget, displayType, updateWidgetSize)
  const availableFonts = useAvailableFonts()
  const definition = WIDGET_TYPE_DEFINITIONS[widget.type]
  const unitOptions = definition.supportedDisplayUnits
  const unitsMode = definition.unitsMode
  const supportsUnitSelection = unitOptions.length > 1
  const cornerRadiusMax = getArcCornerRadiusMax(arcData, displayType)
  const size = widget.data.width ?? arcData.width
  const barGapMax = arcData.track_fill_style === 'bars' ? getArcGapMax(arcData, displayType) : 0

  const updateBoundedNumber = (updateVariant, key, rawValue, min, max) => {
    const value = Number(rawValue)
    if (!Number.isFinite(value)) return
    updateVariant({ [key]: Math.min(max, Math.max(min, value)) })
  }

  const handleSizeChange = (nextSize) => {
    const update = buildUniformResizeUpdate(widget, nextSize)
    updateWidgetSize(widget.id, update)
  }

  return (
    <>
      <div className="space-y-4">
        <SectionHeading
          icon={SlidersHorizontal}
          title={isCornerGauge ? t('widget-editor.cornerTrack', 'Corner Track') : t('widget-editor.arcTrack', 'Arc Track')}
        />
        <div className="grid grid-cols-1 gap-4">
          <SizeSlider
            label={t('widget-editor.size', 'Size')}
            value={size}
            min={30}
            max={600}
            step={1}
            valueDisplay={`${Math.round(size)}`}
            suffix="px"
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
              options={translateOptions(CORNER_ORIENTATION_OPTIONS, t)}
            />
          ) : isGtGauge ? (
            <SliderField
              editable
              label={t('widget-editor.tickLength', 'Tick Length')}
              value={arcData.tick_length}
              min={0}
              max={100}
              step={1}
              integerDisplay
              valueDisplay={`${arcData.tick_length}`}
              suffix="px"
              onSliderChange={(tick_length) => updateArcSize({ tick_length })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          ) : (
            <SliderField
              editable
              label={t('widget-editor.arcAngle', 'Arc Angle')}
              value={arcData.arc_angle}
              min={ARC_MIN_ANGLE}
              max={ARC_MAX_ANGLE}
              step={5}
              valueDisplay={`${arcData.arc_angle}`}
              suffix="°"
              onSliderChange={(arc_angle) => updateArcSize({ arc_angle })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          )}
          <SliderField
            editable
            label={t('widget-editor.thickness', 'Thickness')}
            value={arcData.track_thickness}
            min={1}
            max={100}
            step={1}
            integerDisplay
            valueDisplay={`${arcData.track_thickness}`}
            suffix="px"
            onSliderChange={(track_thickness) =>
              updateArcSize({
                track_thickness,
                track_corner_radius: Math.min(arcData.track_corner_radius, getArcCornerRadiusMax({ ...arcData, track_thickness }, displayType)),
              })
            }
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <BarFillStyleField
            data={arcData}
            suggestBarGeometry={(data) => suggestArcBarGeometry(data, displayType)}
            updateVariant={updateArc}
            disabled={isGtGauge}
          />
          <SliderField
            editable
            label={t('widget-editor.cornerRadius', 'Corner Radius')}
            value={arcData.track_corner_radius}
            min={0}
            max={cornerRadiusMax}
            step={1}
            integerDisplay
            valueDisplay={`${arcData.track_corner_radius}`}
            suffix="px"
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
          {isGtGauge ? (
            <SliderField
              editable
              label={t('widget-editor.glow', 'Glow')}
              value={arcData.glow}
              min={0}
              max={1}
              step={0.05}
              valueScale={100}
              valueDisplay={`${Math.round(arcData.glow * 100)}`}
              suffix="%"
              onSliderChange={(glow) => updateArc({ glow })}
            />
          ) : null}
        </div>
      </div>

      <TrackStyleSection
        data={arcData}
        updateVariant={updateArc}
        updateVariantSize={updateArcSize}
        onCommit={() => commitWidgetSize(widget.id)}
        borderMax={6}
        supportsGradient={!isGtGauge}
      />

      <div className="space-y-4">
        <FontSection
          widget={widget}
          updateWidgetData={updateWidgetData}
          updateWidgetSize={updateWidgetSize}
          commitWidgetSize={commitWidgetSize}
          title={t('widget-editor.label', 'Label')}
          fontSizeLabel={t('widget-editor.fontSize', 'Font Size')}
        />
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.horizontalOffset', 'Horizontal Offset')}
            value={arcData.inner_widget_offset_x}
            min={-50}
            max={50}
            step={1}
            integerDisplay
            valueDisplay={`${arcData.inner_widget_offset_x}`}
            suffix="px"
            onSliderChange={(value) => updateBoundedNumber(updateArcSize, 'inner_widget_offset_x', value, -10_000, 10_000)}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
          <SliderField
            editable
            label={t('widget-editor.verticalOffset', 'Vertical Offset')}
            value={arcData.inner_widget_offset_y}
            min={-50}
            max={50}
            step={1}
            integerDisplay
            valueDisplay={`${arcData.inner_widget_offset_y}`}
            suffix="px"
            onSliderChange={(value) => updateBoundedNumber(updateArcSize, 'inner_widget_offset_y', value, -10_000, 10_000)}
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
      </div>
      {definition.hasDecimalControl ? (
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.decimals', 'Decimals')}
            value={widget.data.decimals}
            min={0}
            max={definition.maxDecimals}
            step={1}
            valueDisplay={String(widget.data.decimals)}
            onSliderChange={(value) => updateWidgetData(widget.id, { decimals: value })}
          />
        </div>
      ) : null}

      {unitsMode !== 'hidden' ? (
        <UnitsControlRow
          title={t('widget-editor.unit', 'Unit')}
          checked={widget.data.show_units}
          onCheckedChange={(show_units) => updateWidgetData(widget.id, { show_units })}
          colorValue={widget.data.unit_color}
          onColorChange={(unit_color) => updateWidgetData(widget.id, { unit_color })}
          selectLabel={t('widget-editor.unit', 'Unit')}
          value={widget.data.display_unit}
          onValueChange={(displayUnit) =>
            updateWidgetData(widget.id, buildMetricUnitUpdate(widget.type, widget.data.starting_altitude, widget.data.display_unit, displayUnit))
          }
          options={supportsUnitSelection ? unitOptions : undefined}
        />
      ) : null}

      <div className="space-y-4">
        <SectionHeading
          icon={Tags}
          title={t('widget-editor.minmaxLabels', 'Min/Max Labels')}
          trailing={
            <ToggleField checked={arcData.show_min_max_labels} onCheckedChange={(show_min_max_labels) => updateArc({ show_min_max_labels })} />
          }
        />
        <div className="grid grid-cols-2 gap-4">
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
            valueDisplay={`${arcData.min_max_label_font_size}`}
            suffix="px"
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
