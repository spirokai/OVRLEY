import { WIDGET_TYPE_DEFINITIONS } from '@/lib/widget/widget-catalog'
import { FontSection, IconSection, UnitsControlRow } from '../widgetEditorSections'
import { SelectField, SliderField, ToggleField } from '../widgetFormControls'
import { buildMetricUnitUpdate } from '@/lib/widget/altitude'
import { useTranslation } from 'react-i18next'
import { translateOptions } from '@/i18n'

const COORDINATE_FORMAT_OPTIONS = [
  { value: 'dms', labelKey: 'widget-editor.coordinateFormatDms', defaultLabel: 'Deg / Min / Sec' },
  { value: 'ddm', labelKey: 'widget-editor.coordinateFormatDdm', defaultLabel: 'Deg / Dec Min' },
]

const BALANCE_FORMAT_OPTIONS = [
  { value: 'percent_label', label: '52%/48%' },
  { value: 'plain', label: '52/48' },
  { value: 'l_prefix', label: 'L52/R48' },
  { value: 'l_suffix', label: '52L/48R' },
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
  const definition = WIDGET_TYPE_DEFINITIONS[widget.type]
  const unitsMode = definition.unitsMode
  const unitOptions = definition.supportedDisplayUnits
  const showUnits = widget.data.show_units
  const supportsUnitSelection = unitOptions.length > 1
  const isDistanceWidget = widget.type === 'distance'
  const isCoordinateWidget = widget.type === 'gps_coordinates'
  const isTotalAscentWidget = widget.type === 'total_ascent'
  const hasBalanceFormat = definition.formatter === 'balance'

  const handleUnitChange = (value) => {
    updateWidgetData(widget.id, buildMetricUnitUpdate(widget.type, widget.data.starting_altitude, widget.data.display_unit, value))
  }

  return (
    <>
      <FontSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        updateWidgetSize={updateWidgetSize}
        commitWidgetSize={commitWidgetSize}
        showContentAlignment
      />

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
            <span className="text-[0.741rem] font-bold uppercase tracking-wider text-muted-foreground">
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
            value={widget.data.display_unit}
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
            <span className="text-[0.741rem] font-bold uppercase tracking-wider text-muted-foreground">
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
              value={widget.data.display_unit}
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
