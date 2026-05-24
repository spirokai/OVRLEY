/**
 * Supports widget editing flows related to metric widget editor.
 */

import { getStandardMetricDefinition, getStandardMetricDisplayUnit, getStandardMetricUnitOptions } from '@/lib/standard-metrics'
import { BALANCE_FORMAT_OPTIONS } from '@/features/widget-preview/utils/formatUtils'
import { FontSection, IconSection, UnitsControlRow } from './widgetEditorSections'
import { ToggleField, SelectField } from './widgetFormControls'
import { useCallback } from 'react'

/**
 * Renders the metric widget editor component.
 *
 * @param {object} props - Component props.
 * @param {*} props.widget - Widget definition being rendered or edited.
 * @param {*} props.updateWidgetData - Value for update widget data.
 * @param {*} props.setNumericField - Value for set numeric field.
 * @returns {JSX.Element} Rendered component output.
 */
export default function MetricWidgetEditor({ widget, updateWidgetData, setNumericField }) {
  const definition = getStandardMetricDefinition(widget.type)
  const unitOptions = getStandardMetricUnitOptions(widget.type)
  const showUnits = widget.data.show_units ?? definition?.showUnitsByDefault ?? false
  const supportsUnitSelection = unitOptions.length > 1
  const hasDecimalControl = definition?.formatter === 'decimal'
  const hasBalanceFormat = definition?.formatter === 'balance'

  const toggleDecimals = useCallback(() => {
    const current = widget.data.decimals ?? 0
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
          value={widget.data.balance_format || 'percent_label'}
          onValueChange={(value) => updateWidgetData(widget.id, { balance_format: value })}
          options={BALANCE_FORMAT_OPTIONS}
        />
      ) : null}

      <IconSection
        widget={widget}
        updateWidgetData={updateWidgetData}
        setNumericField={setNumericField}
        showUnitsToggle
        unitsField={
          <UnitsControlRow
            widget={widget}
            updateWidgetData={updateWidgetData}
            title={supportsUnitSelection ? 'Units' : 'Unit'}
            checked={showUnits}
            onCheckedChange={(checked) => updateWidgetData(widget.id, { show_units: checked })}
            colorValue={widget.data.unit_color || '#ffffff'}
            onColorChange={(value) => updateWidgetData(widget.id, { unit_color: value })}
            selectLabel="Unit"
            value={getStandardMetricDisplayUnit(widget.type, widget.data)}
            onValueChange={(value) => updateWidgetData(widget.id, { display_unit: value })}
            options={supportsUnitSelection ? unitOptions : undefined}
          />
        }
      />
    </>
  )
}
