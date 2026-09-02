import {
  getStandardMetricDefinition,
  getStandardMetricDisplayUnit,
  getStandardMetricUnitOptions,
  getStandardMetricUnitsMode,
} from '@/lib/widget/standard-metrics'
import { BALANCE_FORMAT_OPTIONS } from '@/features/widget-preview/widgets/metric/format'
import { FontSection, IconSection, UnitsControlRow } from '../widgetEditorSections'
import { SelectField, SliderField, ToggleField } from '../widgetFormControls'
import { buildMetricUnitUpdate } from '@/lib/widget/altitude'
import { TYPE_DEFAULTS } from '@/lib/widget/standard-widgets'
import { useTranslation } from 'react-i18next'
import { translateOptions } from '@/i18n'

const COORDINATE_FORMAT_OPTIONS = [
  { value: 'dms', labelKey: 'widget-editor.coordinateFormatDms', defaultLabel: 'Deg / Min / Sec' },
  { value: 'ddm', labelKey: 'widget-editor.coordinateFormatDdm', defaultLabel: 'Deg / Dec Min' },
]

/**
 * Renders text-specific display controls: font, decimals/balance, icon, units.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.setNumericField - Sets a numeric field.
 */
export default function TextDisplaySection({ widget, updateWidgetData, updateWidgetSize, commitWidgetSize, setNumericField }) {
  const { t } = useTranslation()
  const definition = getStandardMetricDefinition(widget.type)
  const unitsMode = getStandardMetricUnitsMode(widget.type)
  const unitOptions = getStandardMetricUnitOptions(widget.type)
  const showUnits = widget.data.show_units
  const supportsUnitSelection = unitOptions.length > 1
  const isDistanceWidget = widget.type === 'distance'
  const isCoordinateWidget = widget.type === 'gps_coordinates'
  const isTotalAscentWidget = widget.type === 'total_ascent'
  const hasDecimalControl = definition?.maxDecimals !== undefined || definition?.formatter === 'decimal' || definition?.formatter === 'temperature'
  const hasBalanceFormat = definition?.formatter === 'balance'
  const maxDecimals = definition?.maxDecimals ?? 1
  const defaultDecimals = TYPE_DEFAULTS[widget.type]?.decimals ?? 1
  const decimals = Number.isFinite(widget.data.decimals) ? Math.min(Math.max(widget.data.decimals, 0), maxDecimals) : defaultDecimals

  const handleUnitChange = (value) => {
    updateWidgetData(
      widget.id,
      buildMetricUnitUpdate(widget.type, widget.data.starting_altitude, getStandardMetricDisplayUnit(widget.type, widget.data), value),
    )
  }

  return (
    <>
      <FontSection widget={widget} updateWidgetData={updateWidgetData} updateWidgetSize={updateWidgetSize} commitWidgetSize={commitWidgetSize} />

      {hasDecimalControl ? (
        <div className="grid grid-cols-2 gap-4">
          <SliderField
            label={t('widget-editor.decimals', 'Decimals')}
            value={decimals}
            min={0}
            max={maxDecimals}
            step={1}
            valueDisplay={String(decimals)}
            onSliderChange={(value) => updateWidgetData(widget.id, { decimals: value })}
          />
        </div>
      ) : null}

      {hasBalanceFormat ? (
        <SelectField
          label={t('widget-editor.balanceFormat', 'Balance Format')}
          value={widget.data.balance_format}
          onValueChange={(value) => updateWidgetData(widget.id, { balance_format: value })}
          options={BALANCE_FORMAT_OPTIONS}
        />
      ) : null}

      {isDistanceWidget ? (
        <div className="grid grid-cols-2 gap-4">
          <div className="flex items-center justify-between py-2">
            <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
              {t('widget-editor.showFullDistance', 'Show Full Distance')}
            </span>
            <ToggleField
              checked={widget.data.show_full_distance ?? true}
              onCheckedChange={(checked) => updateWidgetData(widget.id, { show_full_distance: checked })}
            />
          </div>
        </div>
      ) : null}

      {isCoordinateWidget ? (
        <div className="grid grid-cols-2 gap-4">
          <SelectField
            label={t('widget-editor.format', 'Format')}
            value={getStandardMetricDisplayUnit(widget.type, widget.data)}
            onValueChange={(value) => updateWidgetData(widget.id, { display_unit: value })}
            options={translateOptions(unitOptions, t)}
          />
          <SelectField
            label={t('widget-editor.coordinates', 'Coordinates')}
            value={widget.data.coordinate_format}
            onValueChange={(value) => updateWidgetData(widget.id, { coordinate_format: value })}
            options={translateOptions(COORDINATE_FORMAT_OPTIONS, t)}
          />
        </div>
      ) : null}

      {isTotalAscentWidget ? (
        <div className="grid grid-cols-2 gap-4">
          <div className="flex items-center justify-between py-2">
            <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
              {t('widget-editor.showFullAscent', 'Show Full Ascent')}
            </span>
            <ToggleField
              checked={widget.data.show_full_ascent}
              onCheckedChange={(checked) => updateWidgetData(widget.id, { show_full_ascent: checked })}
            />
          </div>
        </div>
      ) : null}

      <IconSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        setNumericField={setNumericField}
        showUnitsToggle={unitsMode !== 'hidden'}
        unitsField={
          unitsMode !== 'hidden' ? (
            <UnitsControlRow
              widget={widget}
              updateWidgetData={updateWidgetData}
              title={t(supportsUnitSelection ? 'widget-editor.units' : 'widget-editor.unit', supportsUnitSelection ? 'Units' : 'Unit')}
              checked={showUnits}
              onCheckedChange={(checked) => updateWidgetData(widget.id, { show_units: checked })}
              colorValue={widget.data.unit_color}
              onColorChange={(value) => updateWidgetData(widget.id, { unit_color: value })}
              selectLabel={t('widget-editor.unit', 'Unit')}
              value={getStandardMetricDisplayUnit(widget.type, widget.data)}
              onValueChange={handleUnitChange}
              options={isCoordinateWidget ? undefined : supportsUnitSelection ? unitOptions : undefined}
              showToggle={!isCoordinateWidget}
            />
          ) : null
        }
      />
    </>
  )
}
