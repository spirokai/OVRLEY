/**
 * Supports widget editing flows related to elevation widget editor.
 */

import { Map, Mountain, Palette } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { ColorField, NumberField, SelectField, SizeSlider, SliderField, ToggleField } from './widgetFormControls'
import { DimensionsSection } from './widgetEditorSections'
import { getThemeColor } from '@/lib/theme'
import { Label } from '@/components/ui/label'
import { convertAltitudeInputValue } from '@/lib/widget/altitude'
import { useTranslation } from 'react-i18next'

const MARKER_VARIANT_OPTIONS = [
  { value: 'single', label: 'Single Circle' },
  { value: 'ring', label: 'Concentric Ring' },
  { value: 'halo', label: 'Solid Halo' },
]

const ALTITUDE_UNIT_OPTIONS = [
  { value: 'm', label: 'm' },
  { value: 'ft', label: 'ft' },
]

/**
 * Renders the elevation widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @param {*} props.sceneFontSize - Scene fallback font size.
 * @returns {JSX.Element} Rendered component output.
 */
export default function ElevationWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize, setNumericField, sceneFontSize }) {
  const { t } = useTranslation()
  const lineWidth = widget.data.completed_line_width ?? widget.data.remaining_line_width
  const remainingLineOpacity = widget.data.remaining_line_opacity
  const completedAreaOpacity = widget.data.area_completed_opacity
  const remainingAreaOpacity = widget.data.area_remaining_opacity
  const yScale = widget.data.y_scale
  const simplifyTolerance = widget.data.simplify_tolerance_px
  const targetDensity = widget.data.target_density
  const markerSize = widget.data.marker_size
  const markerOpacity = widget.data.marker_opacity
  const markerVariant = widget.data.marker_variant
  const markerVariantDiameter = widget.data.marker_variant_diameter
  const labelFontSize = widget.data.point_label?.font_size ?? sceneFontSize ?? 12.5
  const showVariantDiameter = markerVariant !== 'single'
  const variantDiameterLabel = markerVariant === 'ring' ? 'Ring Diameter' : 'Halo Diameter'
  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} />
      <div className="space-y-4">
        <SectionHeading icon={Palette} title={t('widget-editor.lineStyling', 'Line Styling')} />
        <SliderField
          label="Thickness"
          value={lineWidth}
          min={0}
          max={20}
          step={1}
          integerDisplay
          valueDisplay={`${lineWidth}px`}
          onSliderChange={(value) =>
            updateWidgetSize(widget.id, {
              completed_line_width: value,
              remaining_line_width: value,
            })
          }
          onSliderCommit={() => commitWidgetSize(widget.id)}
        />

        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label="Smoothing"
            value={simplifyTolerance}
            min={0}
            max={4}
            step={0.05}
            valueDisplay={`${simplifyTolerance.toFixed(2)}px`}
            onSliderChange={(value) =>
              updateWidgetSize(widget.id, {
                simplify_tolerance_px: Number(value.toFixed(2)),
              })
            }
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
          <SliderField
            label={t('widget-editor.profileDetail', 'Profile Detail')}
            value={targetDensity}
            min={0.25}
            max={1.5}
            step={0.05}
            valueDisplay={`${targetDensity.toFixed(2)}x`}
            onSliderChange={(value) =>
              updateWidgetSize(widget.id, {
                target_density: Number(value.toFixed(2)),
              })
            }
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.finishedColor', 'Finished Color')}
            value={widget.data.completed_line_color || getThemeColor('ice')}
            onChange={(value) =>
              updateWidgetData(widget.id, {
                completed_line_color: value,
                color: value,
              })
            }
          />
          <ColorField
            label={t('widget-editor.remainingColor', 'Remaining Color')}
            value={widget.data.remaining_line_color || getThemeColor('teal')}
            onChange={(value) => updateWidgetData(widget.id, { remaining_line_color: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label={t('widget-editor.finishedOpacity', 'Finished Opacity')}
            value={widget.data.completed_line_opacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${widget.data.completed_line_opacity}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { completed_line_opacity: value })}
          />
          <SliderField
            label={t('widget-editor.remainingOpacity', 'Remaining Opacity')}
            value={remainingLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${remainingLineOpacity}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { remaining_line_opacity: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label={t('widget-editor.verticalScale', 'Vertical Scale')}
            value={yScale}
            min={0.1}
            max={3}
            step={0.05}
            valueDisplay={`${yScale.toFixed(2)}x`}
            onSliderChange={(value) =>
              updateWidgetSize(widget.id, {
                y_scale: Number(value.toFixed(2)),
              })
            }
            onSliderCommit={() => commitWidgetSize(widget.id)}
          />

          <div className="flex items-center justify-between gap-2 px-1 pt-6">
            <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.showFullActivity', 'Show Full Activity')}</Label>
            <ToggleField
              checked={widget.data.show_full_activity}
              onCheckedChange={(checked) => updateWidgetData(widget.id, { show_full_activity: checked })}
            />
          </div>
        </div>
        <div className="space-y-4">
          <SectionHeading icon={Palette} title={t('widget-editor.areaStyling', 'Area Styling')} />
          <div className="grid grid-cols-2 gap-4">
            <ColorField
              label={t('widget-editor.finishedColor', 'Finished Color')}
              value={widget.data.area_completed_color || getThemeColor('ice')}
              onChange={(value) => updateWidgetData(widget.id, { area_completed_color: value })}
            />
            <ColorField
              label={t('widget-editor.remainingColor', 'Remaining Color')}
              value={widget.data.area_remaining_color || getThemeColor('teal')}
              onChange={(value) => updateWidgetData(widget.id, { area_remaining_color: value })}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <SliderField
              label={t('widget-editor.finishedOpacity', 'Finished Opacity')}
              value={completedAreaOpacity}
              min={0}
              max={100}
              step={1}
              valueDisplay={`${completedAreaOpacity}%`}
              onSliderChange={(value) => updateWidgetData(widget.id, { area_completed_opacity: value })}
            />
            <SliderField
              label={t('widget-editor.remainingOpacity', 'Remaining Opacity')}
              value={remainingAreaOpacity}
              min={0}
              max={100}
              step={1}
              valueDisplay={`${remainingAreaOpacity}%`}
              onSliderChange={(value) => updateWidgetData(widget.id, { area_remaining_opacity: value })}
            />
          </div>
        </div>
      </div>
      <div className="space-y-4">
        <SectionHeading icon={Map} title="Marker" />
        <div className="grid grid-cols-2 gap-4">
          <SelectField
            label={t('widget-editor.markerType', 'Marker Type')}
            value={markerVariant}
            options={MARKER_VARIANT_OPTIONS}
            onValueChange={(value) => updateWidgetData(widget.id, { marker_variant: value })}
          />
          <SizeSlider
            label="Size"
            value={markerSize}
            min={0}
            max={50}
            step={1}
            valueDisplay={`${markerSize}px`}
            onChange={(value) => updateWidgetSize(widget.id, { marker_size: value })}
            onCommit={() => commitWidgetSize(widget.id)}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label="Color"
            value={widget.data.marker_color || getThemeColor('aqua')}
            onChange={(value) => updateWidgetData(widget.id, { marker_color: value })}
          />
          <SliderField
            label="Opacity"
            value={markerOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${markerOpacity}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { marker_opacity: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          {showVariantDiameter ? (
            <SizeSlider
              label={variantDiameterLabel}
              value={markerVariantDiameter}
              min={Math.max(Math.round(markerSize * 2), 4)}
              max={120}
              step={1}
              valueDisplay={`${markerVariantDiameter}px`}
              onChange={(value) => updateWidgetSize(widget.id, { marker_variant_diameter: value })}
              onCommit={() => commitWidgetSize(widget.id)}
            />
          ) : null}
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Mountain} title="Labels" />
        <SizeSlider
          label={t('widget-editor.labelSize', 'Label Size')}
          value={labelFontSize}
          min={5}
          max={50}
          step={1}
          valueDisplay={`${labelFontSize}px`}
          onChange={(value) => updateWidgetSize(widget.id, { point_label: { ...(widget.data.point_label ?? {}), font_size: value } })}
          onCommit={() => commitWidgetSize(widget.id)}
        />

        <div className="grid grid-cols-2 gap-4 pt-2">
          <div className="flex flex-col gap-4">
            <div className="flex justify-between items-center gap-2 pl-1 pb-3">
              <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.metricLabel', 'Metric Label')}</Label>
              <ToggleField
                label={t('widget-editor.labelMetric', 'Label Metric')}
                onCheckedChange={(checked) =>
                  updateWidgetData(widget.id, {
                    show_elevation_metric: checked,
                  })
                }
              />
            </div>
            <SliderField
              label={t('widget-editor.metricOffsetX', 'Metric Offset X')}
              disabled={!widget.data.show_elevation_metric}
              value={widget.data.metric_label_offset_x}
              min={-100}
              max={100}
              step={1}
              integerDisplay
              valueDisplay={`${widget.data.metric_label_offset_x}px`}
              onSliderChange={(value) => updateWidgetSize(widget.id, { metric_label_offset_x: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
            <SliderField
              label={t('widget-editor.metricOffsetY', 'Metric Offset Y')}
              disabled={!widget.data.show_elevation_metric}
              value={widget.data.metric_label_offset_y}
              min={-100}
              max={100}
              step={1}
              integerDisplay
              valueDisplay={`${widget.data.metric_label_offset_y}px`}
              onSliderChange={(value) => updateWidgetSize(widget.id, { metric_label_offset_y: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          </div>
          <div className="flex flex-col gap-4">
            <div className="flex justify-between items-center gap-2 pl-1 pb-3">
              <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.imperialLabel', 'Imperial Label')}</Label>
              <ToggleField
                label={t('widget-editor.labelImperial', 'Label Imperial')}
                checked={widget.data.show_elevation_imperial}
                onCheckedChange={(checked) =>
                  updateWidgetData(widget.id, {
                    show_elevation_imperial: checked,
                  })
                }
              />
            </div>
            <SliderField
              label={t('widget-editor.imperialOffsetX', 'Imperial Offset X')}
              disabled={!widget.data.show_elevation_imperial}
              value={widget.data.imperial_label_offset_x}
              min={-100}
              max={100}
              step={1}
              integerDisplay
              valueDisplay={`${widget.data.imperial_label_offset_x}px`}
              onSliderChange={(value) => updateWidgetSize(widget.id, { imperial_label_offset_x: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
            <SliderField
              label={t('widget-editor.imperialOffsetY', 'Imperial Offset Y')}
              disabled={!widget.data.show_elevation_imperial}
              value={widget.data.imperial_label_offset_y}
              min={-100}
              max={100}
              step={1}
              integerDisplay
              valueDisplay={`${widget.data.imperial_label_offset_y}px`}
              onSliderChange={(value) => updateWidgetSize(widget.id, { imperial_label_offset_y: value })}
              onSliderCommit={() => commitWidgetSize(widget.id)}
            />
          </div>
        </div>
        <div className="grid grid-cols-2 gap-4">
          <NumberField
            label={t('widget-editor.elevationAtStart', 'Elevation at start')}
            value={widget.data.starting_altitude}
            placeholder={widget.startingAltitudePlaceholder}
            onChange={(rawValue) => setNumericField(widget.id, 'starting_altitude', rawValue, { optional: true, round: true })}
          />
          <SelectField
            label=""
            value={widget.data.starting_altitude_unit}
            options={ALTITUDE_UNIT_OPTIONS}
            onReset={() => updateWidgetData(widget.id, { starting_altitude: null })}
            onValueChange={(value) =>
              updateWidgetData(widget.id, {
                starting_altitude: convertAltitudeInputValue(widget.data.starting_altitude, widget.data.starting_altitude_unit, value),
                starting_altitude_unit: value,
              })
            }
          />
        </div>
      </div>
    </>
  )
}
