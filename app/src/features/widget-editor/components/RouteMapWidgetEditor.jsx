/**
 * Supports widget editing flows related to route map widget editor.
 */

import { Map, Palette } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { ColorField, SelectField, SizeSlider, SliderField, ToggleField } from './widgetFormControls'
import { DimensionsSection } from './widgetEditorSections'
import { getThemeColor } from '@/lib/theme'
import { Label } from '@/components/ui/label'
import { useTranslation } from 'react-i18next'

const MARKER_VARIANT_OPTIONS = [
  { value: 'single', label: 'Single Circle' },
  { value: 'ring', label: 'Concentric Ring' },
  { value: 'halo', label: 'Solid Halo' },
]

/**
 * Renders the route map widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function RouteMapWidgetEditor({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize, setNumericField }) {
  const { t } = useTranslation()
  const lineWidth = widget.data.completed_line_width ?? widget.data.remaining_line_width
  const completedLineOpacity = widget.data.completed_line_opacity
  const remainingLineOpacity = widget.data.remaining_line_opacity
  const markerSize = widget.data.marker_size
  const markerOpacity = widget.data.marker_opacity
  const markerVariant = widget.data.marker_variant
  const markerVariantDiameter = widget.data.marker_variant_diameter
  const rotation = widget.data.rotation
  const simplifyTolerance = widget.data.simplify_tolerance_px
  const targetDensity = widget.data.target_density
  const showVariantDiameter = markerVariant !== 'single'
  const variantDiameterLabel = markerVariant === 'ring' ? 'Ring Diameter' : 'Halo Diameter'

  return (
    <>
      <DimensionsSection widget={widget} setNumericField={setNumericField} />

      <SliderField
        label={t('widget-editor.mapRotation', 'Map Rotation')}
        value={rotation}
        min={-180}
        max={180}
        step={1}
        valueDisplay={`${rotation}°`}
        onSliderChange={(rawValue) => updateWidgetSize(widget.id, { rotation: rawValue })}
        onSliderCommit={() => commitWidgetSize(widget.id)}
      />

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
            label={t('widget-editor.routeDetail', 'Route Detail')}
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
            value={completedLineOpacity}
            min={0}
            max={100}
            step={1}
            valueDisplay={`${completedLineOpacity}%`}
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
          <div className="flex items-center justify-between gap-2 pl-1 pt-2 pb-2">
            <Label className="p-0 text-[9px] text-muted-foreground uppercase font-bold">{t('widget-editor.showFullActivity', 'Show Full Activity')}</Label>
            <ToggleField
              checked={widget.data.show_full_activity}
              onCheckedChange={(checked) => updateWidgetData(widget.id, { show_full_activity: checked })}
            />
          </div>
        </div>
      </div>
      <div className="space-y-4">
        <SectionHeading icon={Map} title="Marker" />
        <div className="grid grid-cols-2 gap-4">
          <SelectField
            label="Type"
            value={markerVariant}
            options={MARKER_VARIANT_OPTIONS}
            onValueChange={(value) => updateWidgetData(widget.id, { marker_variant: value })}
          />
          <SizeSlider
            label={t('widget-editor.size', ' Size')}
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
    </>
  )
}
