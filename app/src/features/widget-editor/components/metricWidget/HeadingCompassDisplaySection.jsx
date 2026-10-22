import { useMemo } from 'react'
import { Compass, Hash, Target, Type } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import FontSelectField from '@/components/ui/font-select-field'
import { useAvailableFonts } from '@/hooks/useFonts'
import { useTranslation } from 'react-i18next'
import { buildUniformResizeUpdate } from '../../utils/widgetSize'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { ColorField, FieldBlock, SizeSlider, SliderField, ToggleField } from '../widgetFormControls'

const ROTATION_MODE_OPTIONS = [
  { value: 'indicator', labelKey: 'widget-editor.indicator', defaultLabel: 'Indicator' },
  { value: 'compass', labelKey: 'widget-editor.compass', defaultLabel: 'Compass' },
]

/**
 * Editor controls for the diameter-based heading compass display type.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.updateWidgetSize - Updates size-affecting widget data.
 * @param {Function} props.commitWidgetSize - Commits a size-affecting edit.
 * @returns {JSX.Element}
 */
export default function HeadingCompassDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const { t } = useTranslation()
  const compass = useMemo(() => widget.data.display_variants?.heading_compass ?? {}, [widget.data.display_variants?.heading_compass])
  const updateCompass = useDisplayVariantUpdater(widget, 'heading_compass', updateWidgetData)
  const updateCompassSize = useDisplayVariantUpdater(widget, 'heading_compass', updateWidgetSize)
  const availableFonts = useAvailableFonts()
  const commit = () => commitWidgetSize(widget.id)

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={Compass} title={t('widget-editor.compassDial', 'Dial')} />
        <FieldBlock label={t('widget-editor.rotationMode', 'Rotation Mode')}>
          <ToggleGroup
            type="single"
            className="w-full"
            value={compass.rotation_mode}
            onValueChange={(rotation_mode) => {
              if (rotation_mode) updateCompass({ rotation_mode })
            }}
          >
            {ROTATION_MODE_OPTIONS.map((option) => (
              <ToggleGroupItem key={option.value} value={option.value} className="h-7 w-full px-3 text-xs">
                {t(option.labelKey, option.defaultLabel)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </FieldBlock>
        <div className="grid grid-cols-2 gap-4">
          <SizeSlider
            label={t('widget-editor.size', 'Size')}
            value={compass.diameter}
            min={60}
            max={800}
            step={1}
            valueDisplay={`${Math.round(compass.diameter)}`}
            suffix="px"
            onChange={(diameter) => updateWidgetSize(widget.id, buildUniformResizeUpdate(widget, diameter))}
            onCommit={commit}
          />
          <ColorField
            label={t('widget-editor.trackFill', 'Track Fill')}
            value={compass.track_fill}
            onChange={(track_fill) => updateCompass({ track_fill })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.thickness', 'Thickness')}
            value={compass.track_thickness}
            min={1}
            max={60}
            step={1}
            integerDisplay
            valueDisplay={`${compass.track_thickness}`}
            suffix="px"
            onSliderChange={(track_thickness) => updateCompassSize({ track_thickness })}
            onSliderCommit={commit}
          />
          <SliderField
            editable
            label={t('widget-editor.cornerRadius', 'Corner Radius')}
            value={compass.track_corner_radius}
            min={0}
            max={Math.floor(compass.track_thickness / 2)}
            step={1}
            integerDisplay
            valueDisplay={`${compass.track_corner_radius}`}
            suffix="px"
            onSliderChange={(track_corner_radius) => updateCompass({ track_corner_radius })}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Target} title={t('widget-editor.indicator', 'Indicator')} />
        <div className="grid grid-cols-2 gap-4">
          <SizeSlider
            label={t('widget-editor.indicatorSize', 'Indicator Size')}
            value={compass.indicator_size}
            min={8}
            max={Math.round(compass.diameter)}
            step={1}
            valueDisplay={`${Math.round(compass.indicator_size)}`}
            suffix="px"
            onChange={(indicator_size) => updateCompassSize({ indicator_size })}
            onCommit={commit}
          />
          <SliderField
            editable
            label={t('widget-editor.cornerRadius', 'Corner Radius')}
            value={compass.indicator_corner_radius}
            min={0}
            max={100}
            step={1}
            integerDisplay
            valueDisplay={`${compass.indicator_corner_radius}`}
            suffix="%"
            onSliderChange={(indicator_corner_radius) => updateCompass({ indicator_corner_radius })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.indicatorColor', 'Indicator Color')}
            value={compass.indicator_color}
            onChange={(indicator_color) => updateCompass({ indicator_color })}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading
          icon={Type}
          title={t('widget-editor.cardinalLabels', 'Cardinal Labels')}
          trailing={
            <ToggleField
              label={t('widget-editor.showLabels', 'Show Labels')}
              checked={compass.show_cardinal_labels}
              onCheckedChange={(show_cardinal_labels) => updateCompassSize({ show_cardinal_labels })}
            />
          }
        />
        <SizeSlider
          label={t('widget-editor.fontSize', 'Font Size')}
          value={compass.cardinal_font_size}
          min={6}
          max={120}
          disabled={!compass.show_cardinal_labels}
          valueDisplay={`${Math.round(compass.cardinal_font_size)}`}
          suffix="px"
          onChange={(cardinal_font_size) => updateCompassSize({ cardinal_font_size })}
          onCommit={commit}
        />
        <div className="grid grid-cols-2 gap-4">
          <FontSelectField
            label={t('widget-editor.font', 'Font')}
            value={widget.data.font}
            onValueChange={(font) => updateWidgetData(widget.id, { font })}
            recommendedFonts={availableFonts.recommendedFonts}
            systemFonts={availableFonts.systemFonts}
            triggerClassName="h-9 border-border/70 bg-surface text-xs"
            labelClassName="text-[9px] text-muted-foreground uppercase font-bold"
          />
          <ColorField
            label={t('widget-editor.labelColor', 'Label Color')}
            value={compass.cardinal_label_color}
            disabled={!compass.show_cardinal_labels}
            onChange={(cardinal_label_color) => updateCompass({ cardinal_label_color })}
          />
        </div>
      </div>

      <div className="space-y-4">
        <SectionHeading
          icon={Hash}
          title={t('widget-editor.headingValue', 'Value')}
          trailing={
            <ToggleField
              label={t('widget-editor.showValue', 'Show Value')}
              checked={compass.show_value}
              onCheckedChange={(show_value) => updateCompassSize({ show_value })}
            />
          }
        />
        <SizeSlider
          label={t('widget-editor.fontSize', 'Font Size')}
          value={widget.data.font_size}
          min={6}
          max={200}
          disabled={!compass.show_value}
          valueDisplay={`${Math.round(widget.data.font_size)}`}
          suffix="px"
          onChange={(font_size) => updateWidgetSize(widget.id, { font_size })}
          onCommit={commit}
        />
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.valueColor', 'Value Color')}
            value={widget.data.color}
            disabled={!compass.show_value}
            onChange={(color) => updateWidgetData(widget.id, { color })}
          />
          <ColorField
            label={t('widget-editor.unitColor', 'Unit Color')}
            value={widget.data.unit_color}
            disabled={!compass.show_value}
            onChange={(unit_color) => updateWidgetData(widget.id, { unit_color })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.horizontalOffset', 'Horizontal Offset')}
            value={compass.value_offset_x}
            min={-200}
            max={200}
            step={1}
            integerDisplay
            disabled={!compass.show_value}
            valueDisplay={`${compass.value_offset_x}`}
            suffix="px"
            onSliderChange={(value_offset_x) => updateCompassSize({ value_offset_x })}
            onSliderCommit={commit}
          />
          <SliderField
            editable
            label={t('widget-editor.verticalOffset', 'Vertical Offset')}
            value={compass.value_offset_y}
            min={-200}
            max={200}
            step={1}
            integerDisplay
            disabled={!compass.show_value}
            valueDisplay={`${compass.value_offset_y}`}
            suffix="px"
            onSliderChange={(value_offset_y) => updateCompassSize({ value_offset_y })}
            onSliderCommit={commit}
          />
        </div>
      </div>
    </>
  )
}
