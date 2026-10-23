import { useMemo } from 'react'
import { HeartPulse, Target, Type } from 'lucide-react'
import { SectionHeading } from '@/components/ui/section-heading'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import FontSelectField from '@/components/ui/font-select-field'
import { useAvailableFonts } from '@/hooks/useFonts'
import { useTranslation } from 'react-i18next'
import useDisplayVariantUpdater from '../../hooks/useDisplayVariantUpdater'
import { buildZoneCalculationUpdate, updateManualZone } from '../../utils/heartZones'
import { ColorField, NumberField, SelectField, SizeSlider, SliderField, ToggleField } from '../widgetFormControls'

const ZONE_CALCULATION_OPTIONS = [
  { value: 'lthr', labelKey: 'widget-editor.zoneCalculationLthr', defaultLabel: 'Lactate Threshold HR' },
  { value: 'max_heartrate', labelKey: 'widget-editor.zoneCalculationMaxHeartrate', defaultLabel: 'Max Heart Rate' },
  { value: 'manual', labelKey: 'widget-editor.zoneCalculationManual', defaultLabel: 'Manual' },
]

const INDICATOR_STYLE_OPTIONS = [
  { value: 'line', labelKey: 'widget-editor.indicatorLine', defaultLabel: 'Line' },
  { value: 'arrow', labelKey: 'widget-editor.indicatorArrow', defaultLabel: 'Arrow' },
]

/**
 * Editor controls for the five-zone heart-rate track display type.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.updateWidgetSize - Updates size-affecting widget data.
 * @param {Function} props.commitWidgetSize - Commits a size-affecting edit.
 * @returns {JSX.Element}
 */
export default function HeartZonesDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize }) {
  const { t } = useTranslation()
  const zones = useMemo(() => widget.data.display_variants?.heart_zones ?? {}, [widget.data.display_variants?.heart_zones])
  const updateZones = useDisplayVariantUpdater(widget, 'heart_zones', updateWidgetData)
  const updateZonesSize = useDisplayVariantUpdater(widget, 'heart_zones', updateWidgetSize)
  const availableFonts = useAvailableFonts()
  const commit = () => commitWidgetSize(widget.id)
  const zoneCalculationOptions = ZONE_CALCULATION_OPTIONS.map((option) => ({ value: option.value, label: t(option.labelKey, option.defaultLabel) }))
  const maxCornerRadius = Math.floor(Math.min((zones.width - zones.zone_gap * 4) / 5, zones.height) / 2)

  return (
    <>
      <div className="space-y-4">
        <SectionHeading icon={HeartPulse} title={t('widget-editor.heartZones', 'Zones')} />
        <div className="grid grid-cols-2 gap-4">
          <SizeSlider
            label={t('widget-editor.width', 'Width')}
            value={zones.width}
            min={60}
            max={1600}
            valueDisplay={`${Math.round(zones.width)}`}
            suffix="px"
            onChange={(width) => updateZonesSize({ width })}
            onCommit={commit}
          />
          <SizeSlider
            label={t('widget-editor.height', 'Height')}
            value={zones.height}
            min={8}
            max={200}
            valueDisplay={`${Math.round(zones.height)}`}
            suffix="px"
            onChange={(height) => updateZonesSize({ height })}
            onCommit={commit}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            editable
            label={t('widget-editor.cornerRadius', 'Corner Radius')}
            value={Math.min(zones.track_corner_radius, maxCornerRadius)}
            min={0}
            max={maxCornerRadius}
            step={1}
            integerDisplay
            suffix="px"
            onSliderChange={(track_corner_radius) => updateZones({ track_corner_radius })}
          />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <SelectField
            label={t('widget-editor.zoneCalculation', 'Zone Calculation')}
            value={zones.zone_calculation}
            options={zoneCalculationOptions}
            onValueChange={(zone_calculation) => updateZones(buildZoneCalculationUpdate(zones, zone_calculation))}
          />
          {zones.zone_calculation === 'lthr' ? (
            <NumberField
              label={t('widget-editor.lthr', 'Lactate Threshold HR')}
              value={zones.lthr}
              min={1}
              suffix="bpm"
              onChange={(value) => updateZones({ lthr: Math.max(Number(value), 1) })}
            />
          ) : null}
          {zones.zone_calculation === 'max_heartrate' ? (
            <NumberField
              label={t('widget-editor.maxHeartrate', 'Max Heart Rate')}
              value={zones.max_heartrate}
              min={1}
              suffix="bpm"
              onChange={(value) => updateZones({ max_heartrate: Math.max(Number(value), 1) })}
            />
          ) : null}
        </div>
        {zones.zone_calculation === 'manual'
          ? zones.manual_zones.map((zone, index) => (
              <div key={index} className="grid grid-cols-2 gap-4">
                {['from', 'to'].map((bound) => (
                  <NumberField
                    key={bound}
                    label={`Z${index + 1} ${bound === 'from' ? t('widget-editor.zoneFrom', 'From') : t('widget-editor.zoneTo', 'To')}`}
                    value={zone[bound]}
                    min={0}
                    suffix="bpm"
                    onChange={(value) => updateZones({ manual_zones: updateManualZone(zones.manual_zones, index, bound, Number(value)) })}
                  />
                ))}
              </div>
            ))
          : null}
      </div>

      <div className="space-y-4">
        <SectionHeading
          icon={Type}
          title={t('widget-editor.zoneLabel', 'Zone Label')}
          trailing={
            <ToggleField
              label={t('widget-editor.zoneLabel', 'Zone Label')}
              checked={zones.show_label}
              onCheckedChange={(show_label) => updateZones({ show_label })}
            />
          }
        />
        <FontSelectField
          label={t('widget-editor.font', 'Font')}
          value={widget.data.font}
          disabled={!zones.show_label}
          onValueChange={(font) => updateWidgetData(widget.id, { font })}
          recommendedFonts={availableFonts.recommendedFonts}
          systemFonts={availableFonts.systemFonts}
          triggerClassName="h-9 border-border/70 bg-surface text-xs"
          labelClassName="text-[0.667rem] text-muted-foreground uppercase font-bold"
        />
        <ColorField
          label={t('widget-editor.fontColor', 'Font Color')}
          value={zones.label_color}
          disabled={!zones.show_label}
          onChange={(label_color) => updateZones({ label_color })}
        />
      </div>

      <div className="space-y-4">
        <SectionHeading icon={Target} title={t('widget-editor.indicator', 'Indicator')} />
        <ToggleGroup
          type="single"
          className="w-full"
          value={zones.indicator_style}
          onValueChange={(indicator_style) => {
            if (indicator_style) updateZones({ indicator_style })
          }}
        >
          {INDICATOR_STYLE_OPTIONS.map((option) => (
            <ToggleGroupItem key={option.value} value={option.value} className="h-7 w-full px-3 text-xs">
              {t(option.labelKey, option.defaultLabel)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <div className="grid grid-cols-2 gap-4">
          <ColorField
            label={t('widget-editor.indicatorColor', 'Indicator Color')}
            value={zones.indicator_color}
            onChange={(indicator_color) => updateZones({ indicator_color })}
          />
          <SizeSlider
            label={t('widget-editor.indicatorSize', 'Indicator Size')}
            value={zones.indicator_size}
            min={4}
            max={80}
            disabled={zones.indicator_style !== 'arrow'}
            valueDisplay={`${Math.round(zones.indicator_size)}`}
            suffix="px"
            onChange={(indicator_size) => updateZonesSize({ indicator_size })}
            onCommit={commit}
          />
        </div>
      </div>
    </>
  )
}
