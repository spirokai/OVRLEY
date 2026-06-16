import {
  getStandardMetricDefinition,
  getStandardMetricDisplayUnit,
  getStandardMetricUnitOptions,
  getStandardMetricUnitsMode,
} from '@/lib/widget/standard-metrics'
import { BALANCE_FORMAT_OPTIONS } from '@/features/widget-preview/utils/formatUtils'
import { FontSection, IconSection, UnitsControlRow } from '../widgetEditorSections'
import { ToggleField, SelectField } from '../widgetFormControls'
import { useCallback } from 'react'

/**
 * Renders text-specific display controls: font, decimals/balance, icon, units.
 *
 * @param {object} props
 * @param {object} props.widget - Widget config.
 * @param {Function} props.updateWidgetData - Updates widget data.
 * @param {Function} props.setNumericField - Sets a numeric field.
 */
export default function TextDisplaySection({ widget, updateWidgetData, setNumericField }) {
  const definition = getStandardMetricDefinition(widget.type)
  const unitsMode = getStandardMetricUnitsMode(widget.type)
  const unitOptions = getStandardMetricUnitOptions(widget.type)
  const showUnits = widget.data.show_units
  const supportsUnitSelection = unitOptions.length > 1
  const hasDecimalControl = definition?.formatter === 'decimal' || definition?.formatter === 'temperature'
  const hasBalanceFormat = definition?.formatter === 'balance'

  const toggleDecimals = useCallback(() => {
    const current = widget.data.decimals
    updateWidgetData(widget.id, { decimals: current === 0 ? 1 : 0 })
  }, [widget.id, widget.data.decimals, updateWidgetData])

  return (
    <>
      <FontSection widget={widget} updateWidgetData={updateWidgetData} />

      {hasDecimalControl ? (
        <div className="flex items-center justify-between py-2">
          <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Decimals</span>
          <ToggleField checked={Boolean(widget.data.decimals)} onCheckedChange={toggleDecimals} />
        </div>
      ) : null}

      {hasBalanceFormat ? (
        <SelectField
          label="Balance Format"
          value={widget.data.balance_format}
          onValueChange={(value) => updateWidgetData(widget.id, { balance_format: value })}
          options={BALANCE_FORMAT_OPTIONS}
        />
      ) : null}

      <IconSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        setNumericField={setNumericField}
        showUnitsToggle={unitsMode !== 'hidden'}
        unitsField={
          unitsMode !== 'hidden' ? (
            <UnitsControlRow
              widget={widget}
              updateWidgetData={updateWidgetData}
              title={supportsUnitSelection ? 'Units' : 'Unit'}
              checked={showUnits}
              onCheckedChange={(checked) => updateWidgetData(widget.id, { show_units: checked })}
              colorValue={widget.data.unit_color}
              onColorChange={(value) => updateWidgetData(widget.id, { unit_color: value })}
              selectLabel="Unit"
              value={getStandardMetricDisplayUnit(widget.type, widget.data)}
              onValueChange={(value) => updateWidgetData(widget.id, { display_unit: value })}
              options={supportsUnitSelection ? unitOptions : undefined}
            />
          ) : null
        }
      />
    </>
  )
}
