/**
 * Supports widget editing flows related to elevation widget editor.
 */

import { Mountain, Palette } from 'lucide-react'
import { ColorField, SelectField, SliderField, ToggleField } from './widgetFormControls'
import { DimensionsSection, SectionHeading } from './widgetEditorSections'
import { getThemeColor } from '@/lib/theme'
import { Label } from '@/components/ui/label'

const MARKER_VARIANT_OPTIONS = [
  { value: 'single', label: 'Single Circle' },
  { value: 'ring', label: 'Concentric Ring' },
  { value: 'halo', label: 'Solid Halo' },
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
export default function ElevationWidgetEditor({ widget, updateWidgetData, setNumericField, sceneFontSize }) {
  const lineWidth = widget.data.completed_line_width ?? widget.data.remaining_line_width
  const completedLineOpacity = widget.data.completed_line_opacity
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
  const updatePointLabel = (updates) =>
    updateWidgetData(widget.id, {
      point_label: {
        ...(widget.data.point_label ?? {}),
        ...updates,
      },
    })

  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} />
      <div className="space-y-4">
        <SectionHeading icon={Palette} title="Line Styling" />
        <div className="flex items-center justify-between gap-2 px-1">
          <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">Render Full Activity</Label>
          <ToggleField
            checked={widget.data.show_full_activity}
            onCheckedChange={(checked) => updateWidgetData(widget.id, { show_full_activity: checked })}
          />
        </div>
        <SliderField
          label="Thickness"
          value={lineWidth}
          min={0}
          max={20}
          step={1}
          valueDisplay={`${lineWidth}px`}
          onSliderChange={(value) =>
            updateWidgetData(widget.id, {
              completed_line_width: value,
              remaining_line_width: value,
            })
          }
        />
        <div className="grid grid-cols-2 gap-3">
          <ColorField
            label="Finished Color"
            value={widget.data.completed_line_color || getThemeColor('ice')}
            onChange={(value) =>
              updateWidgetData(widget.id, {
                completed_line_color: value,
                color: value,
              })
            }
          />
          <ColorField
            label="Remaining Color"
            value={widget.data.remaining_line_color || getThemeColor('teal')}
            onChange={(value) => updateWidgetData(widget.id, { remaining_line_color: value })}
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Finished Opacity"
            value={completedLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${completedLineOpacity}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { completed_line_opacity: value })}
          />
          <SliderField
            label="Remaining Opacity"
            value={remainingLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${remainingLineOpacity}%`}
            onSliderChange={(value) => updateWidgetData(widget.id, { remaining_line_opacity: value })}
          />
        </div>
        <SliderField
          label="Vertical Scale"
          value={yScale}
          min={0.1}
          max={3}
          step={0.05}
          valueDisplay={`${yScale.toFixed(2)}x`}
          onSliderChange={(value) =>
            updateWidgetData(widget.id, {
              y_scale: Number(value.toFixed(2)),
            })
          }
        />
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Smoothing"
            value={simplifyTolerance}
            min={0}
            max={4}
            step={0.05}
            valueDisplay={`${simplifyTolerance.toFixed(2)}px`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, {
                simplify_tolerance_px: Number(value.toFixed(2)),
              })
            }
          />
          <SliderField
            label="Data Density"
            value={targetDensity}
            min={0.25}
            max={1.5}
            step={0.05}
            valueDisplay={`${targetDensity.toFixed(2)}x`}
            onSliderChange={(value) =>
              updateWidgetData(widget.id, {
                target_density: Number(value.toFixed(2)),
              })
            }
          />
        </div>
        <div className="space-y-4">
          <SectionHeading icon={Palette} title="Area Styling" />
          <div className="grid grid-cols-2 gap-3">
            <ColorField
              label="Finished Color"
              value={widget.data.area_completed_color || getThemeColor('ice')}
              onChange={(value) => updateWidgetData(widget.id, { area_completed_color: value })}
            />
            <ColorField
              label="Remaining Color"
              value={widget.data.area_remaining_color || getThemeColor('teal')}
              onChange={(value) => updateWidgetData(widget.id, { area_remaining_color: value })}
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <SliderField
              label="Finished Opacity"
              value={completedAreaOpacity}
              min={0}
              max={100}
              step={1}
              valueDisplay={`${completedAreaOpacity}%`}
              onSliderChange={(value) => updateWidgetData(widget.id, { area_completed_opacity: value })}
            />
            <SliderField
              label="Remaining Opacity"
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
        <SectionHeading icon={Mountain} title="Marker & Labels" />
        <SelectField
          label="Marker Type"
          value={markerVariant}
          options={MARKER_VARIANT_OPTIONS}
          onValueChange={(value) => updateWidgetData(widget.id, { marker_variant: value })}
        />
        <div className="grid grid-cols-2 gap-3">
          <SliderField
            label="Size"
            value={markerSize}
            min={0}
            max={50}
            step={1}
            valueDisplay={`${markerSize}px`}
            onSliderChange={(value) => updateWidgetData(widget.id, { marker_size: value })}
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
          <ColorField
            label="Color"
            value={widget.data.marker_color || getThemeColor('aqua')}
            onChange={(value) => updateWidgetData(widget.id, { marker_color: value })}
          />
        </div>
        {showVariantDiameter ? (
          <SliderField
            label={variantDiameterLabel}
            value={markerVariantDiameter}
            min={Math.max(Math.round(markerSize * 2), 4)}
            max={120}
            step={1}
            valueDisplay={`${markerVariantDiameter}px`}
            onSliderChange={(value) => updateWidgetData(widget.id, { marker_variant_diameter: value })}
          />
        ) : null}
        <SliderField
          label="Label Size"
          value={labelFontSize}
          min={5}
          max={50}
          step={1}
          valueDisplay={`${labelFontSize}px`}
          onSliderChange={(value) => updatePointLabel({ font_size: value })}
        />

        <div className="grid grid-cols-2 gap-4 pt-2">
          <div className="flex flex-col gap-4">
            <div className="flex justify-between items-center gap-2 px-1">
              <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">Metric Label</Label>
              <ToggleField
                label="Label Metric"
                checked={widget.data.show_elevation_metric}
                onCheckedChange={(checked) =>
                  updateWidgetData(widget.id, {
                    show_elevation_metric: checked,
                  })
                }
              />
            </div>
            <SliderField
              label="Metric Offset X"
              disabled={!widget.data.show_elevation_metric}
              value={widget.data.metric_label_offset_x}
              min={-100}
              max={100}
              step={1}
              valueDisplay={`${widget.data.metric_label_offset_x}px`}
              onSliderChange={(value) => updateWidgetData(widget.id, { metric_label_offset_x: value })}
            />
            <SliderField
              label="Metric Offset Y"
              disabled={!widget.data.show_elevation_metric}
              value={widget.data.metric_label_offset_y}
              min={-100}
              max={100}
              step={1}
              valueDisplay={`${widget.data.metric_label_offset_y}px`}
              onSliderChange={(value) => updateWidgetData(widget.id, { metric_label_offset_y: value })}
            />
          </div>
          <div className="flex flex-col gap-4">
            <div className="flex justify-between items-center gap-2 px-1">
              <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">Imperial Label</Label>
              <ToggleField
                label="Label Imperial"
                checked={widget.data.show_elevation_imperial}
                onCheckedChange={(checked) =>
                  updateWidgetData(widget.id, {
                    show_elevation_imperial: checked,
                  })
                }
              />
            </div>
            <SliderField
              label="Imperial Offset X"
              disabled={!widget.data.show_elevation_imperial}
              value={widget.data.imperial_label_offset_x}
              min={-100}
              max={100}
              step={1}
              valueDisplay={`${widget.data.imperial_label_offset_x}px`}
              onSliderChange={(value) => updateWidgetData(widget.id, { imperial_label_offset_x: value })}
            />
            <SliderField
              label="Imperial Offset Y"
              disabled={!widget.data.show_elevation_imperial}
              value={widget.data.imperial_label_offset_y}
              min={-100}
              max={100}
              step={1}
              valueDisplay={`${widget.data.imperial_label_offset_y}px`}
              onSliderChange={(value) => updateWidgetData(widget.id, { imperial_label_offset_y: value })}
            />
          </div>
        </div>
      </div>
    </>
  )
}
